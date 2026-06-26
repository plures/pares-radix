# ADR-0025: Hyperswarm-Backed Git Forge (`hyperswarm-git` plugin)

## Status: Proposed

## Date: 2026-06-26

> Driver: kbristol directive (Radix Plugin Program, Task 2.A —
> `workspace/memory/plan-radix-plugins-2026-06-26.md` §2.A). Design-only (Pillar 1).
> Authors the forge architecture + the `git-repo@1.x` CID + a capability-gap list that
> feeds Task 3. **No implementation code in this ADR.** It composes existing foundation
> primitives (PluresDB + its built-in Hyperswarm sync, `plures-object`, `pares-arca`,
> `secrets@1.x` from vault) rather than reinventing storage or transport, per
> C-PLURES-002/004.

## Context

There is **no git forge in the plures org** (verified: GitHub `plures` org + local
`C:\Projects` scan, 2026-06-26). A Hyperswarm-backed forge is genuinely net-new, but it\nis **not greenfield-from-zero**: a git repository *is* a content-addressed object database
with a thin pointer (refs) layer, and the org already ships every substrate it needs:

- **PluresDB** — CRDT store with **built-in Hyperswarm sync (a foundation pillar)**,
  reactive procedures, event triggers, graph ops. The forge's HA/DR rides this sync; it is
  **not** a new transport.
- **`plures-object`** — S3-compatible, content-addressed chunk store with Hyperswarm P2P
  replication. A git object store is content-addressed by construction (SHA-1/SHA-256 of
  the object) → this is the natural home for git blobs/trees/commits/packs.
- **`pares-arca`** — distributed content cache (PluresDB + Hyperswarm) → edge/CDN for
  read-heavy `fetch`/`clone` traffic.
- **`secrets@1.x`** (vault provider, per ADR-0024 §3) — deploy keys, push tokens, SSH host
  keys. The forge **depends on** this; it does not build credential storage.
- **`agens`** (PRIVATE) — optional LLM for PR summarization/review. Optional capability;
  absent ⇒ degrade.

This ADR builds on ADR-0024 (canonical plugin format: `plugin.toml` + `.px` procedures +
adapter IO + `ui/` on design-dojo + capability deps) and ADR-0022 (capability/CID model,
TOML CIDs, mediated events/nodes). The forge is authored as **one canonical plugin** that
**provides** `git-repo@1.x` and **consumes** `storage`, `network`, `secrets@^1.0`, and
optionally `llm`.

**The four problems this design must resolve (named in the program plan as open):**

1. **Object model** — map git loose/packed objects onto `plures-object` content-addressed
   chunks vs. native PluresDB blob nodes.
2. **Wire protocol surface** — which git transports for v1, and the fact that they are
   **long-lived/streaming**, which the current radix IO-actor model (request/response) does
   not serve.
3. **The hard problem** — git refs are single-writer-ish pointers, but PluresDB is
   CRDT/multi-writer. A ref-update conflict policy must be **decided**, not hand-waved.
4. **Feature scope** — what ships in v1 vs. later, mapped to `.px` procedures + mediated
   events.

## Decision

Author `hyperswarm-git` as a canonical ADR-0024 plugin providing `git-repo@1.x`. The
decisions below resolve the four problems and pin the layering: **git *semantics* (packfile
negotiation, ref advertisement, the transports) live in `adapter/` TypeScript (or a Rust
actor) at the IO boundary; forge *logic* (PR / issue / review / release / webhook
lifecycle, ref-update policy) lives in `.px` procedures over PluresDB; UI lives in `ui/` on
design-dojo.**

### 1. Object model — `plures-object` for content, PluresDB nodes for refs/metadata

**RECOMMENDED:** store git **objects and packs in `plures-object`** (content-addressed
chunks); store **refs and all forge metadata as PluresDB nodes**. Justification:

- A git object is *immutable and content-addressed by its hash* — identical to
  `plures-object`'s model. Reusing it gives us de-dup, chunked transfer, and **Hyperswarm
  P2P replication of objects for free**, which is exactly the HA/DR story. Re-encoding git
  objects as PluresDB blob nodes would duplicate a content-addressed store we already own
  (C-PLURES-002 violation) and lose `plures-object`'s replication.
- Refs, PRs, issues, reviews, releases, webhooks are **mutable, queryable, reactive,
  multi-writer** records → exactly PluresDB nodes (so `.px` procedures react to them and
  CRDT sync replicates them). They are small pointers/metadata, not bulk content.
- `git_object` and `pack` therefore appear in the CID as **node *descriptors* that
  reference `plures-object` content by hash** (the node holds `oid` + `kind` + a
  content-address handle; the bytes live in `plures-object`). The node is the index entry;
  `plures-object` is the bytes. `pares-arca` caches hot objects/packs at the edge for
  `fetch`.

Loose vs. packed: incoming loose objects (from `receive-pack`) are written to
`plures-object`; periodic repacking (a `.px`-triggered maintenance op invoking a `git`
actor at the IO boundary) coalesces them into packs, also stored in `plures-object`. Pack
**integrity** (object count + checksum trailer) is a CID invariant the provider must verify
on ingest.

### 2. Wire protocol surface — smart-HTTP first; `git://` + SSH next. Streaming is a real gap.

**v1 transport: smart-HTTP** (`git-upload-pack` for fetch/clone, `git-receive-pack` for
push) — it works through firewalls/proxies, is the most widely supported, and any `git`
client speaks it unmodified. **v2 transports: `git://` daemon, then SSH.**

The **git semantics layer** — ref advertisement, packfile negotiation
(`want`/`have`/`ACK`/`NAK`), pack generation and ingest — lives in **`adapter/` (TS, or a
Rust actor)** at the declared IO boundary (ADR-0024 three-way split). It is pure git
protocol mechanics; it calls into `plures-object` for object bytes and writes/reads ref
nodes via the host. The **forge logic** (who may push, ref-update policy, PR/issue/review
lifecycle) is `.px`.

**🔴 CAPABILITY GAP (feeds Task 3 §3.B):** git smart-HTTP, `git://`, and SSH are
**long-lived, duplex, streaming** protocols (a single fetch can stream a multi-megabyte
packfile; `receive-pack` streams the client's pack in). **Radix's current IO-actor model is
request/response** (mediated events carry a request, a result comes back). It has **no
streaming/duplex transport host capability today.** This is a *real* gap, not a forge-only
hack: the correct fix per the program's governing rule is a **general streaming-transport
host capability** (a `network`-class long-lived/duplex actor surface) usable by any plugin
needing streaming IO, not a git-specific bolt-on. v1 of the forge can begin with a
buffered/chunked request/response approximation for small repos, but the production
transport requires this host capability to land. **This ADR's primary Task-3 output is that
gap.**

### 3. The hard problem — ref-update conflict policy under CRDT (per-repo authoritative writer + CAS)

Git refs are single-writer-ish pointers (`refs/heads/main` is one SHA at a time, advanced
by a push that asserts its expected old value). PluresDB is CRDT/multi-writer — a naive
last-writer-wins on a ref node would silently lose pushes and break git's fundamental
fast-forward guarantee. **Decision (the riskiest design point, made explicit):**

- **Compare-and-swap (CAS) semantics on every ref update.** A ref node carries
  `(repo_id, name, target_oid, generation)`. A push's `receive-pack` supplies the
  **expected old `oid`** (git already sends this in the ref-update command). The provider's
  `.px` ref-update procedure accepts the update **iff** the stored `target_oid` equals the
  expected old oid (or the ref is being created and is absent), then atomically advances
  `target_oid` and bumps `generation`. Mismatch ⇒ **reject with non-fast-forward** (the
  exact error git already understands), surfaced to the client. This makes ref updates
  *linearizable per ref* and maps native git semantics onto the store. **This is a CID
  invariant (`ref_update_cas`).**
- **Per-repo authoritative writer to serialize concurrent CAS.** Because CAS across a
  CRDT needs a serialization point, each repo binds to a **Hyperswarm topic = repo_id**, and
  ref-mutating writes are funneled through an **authoritative writer for that repo** (a
  per-repo single-writer election over the Hyperswarm topic). Reads/clones/fetches are
  served by **any** replica (that is the HA/DR win — full P2P read scale-out); only the
  small, rare ref-*mutation* path is serialized through the elected writer. Writer
  unavailability ⇒ re-election; until a writer is present, the repo is **read-only**
  (fetch/clone keep working from replicas), which is a safe, explicit degradation rather
  than divergent refs.
- **Why this and not pure-CRDT refs:** a pure-CRDT ref would have to *merge* two divergent
  branch tips, which has no correct git meaning (it would fabricate history). CAS + a
  per-repo writer preserves git's "a push either fast-forwards or is rejected" contract
  exactly, while still getting CRDT replication for everything that legitimately *is*
  multi-writer (issues, PR comments, reviews — those merge fine). **Objects are immutable and
  content-addressed**, so object replication needs no conflict policy at all; only the
  pointer (ref) layer does.

This split — **immutable content (P2P, no conflict) + serialized pointers (CAS via per-repo
writer) + freely-merging social metadata (CRDT)** — is the core architectural insight of the
forge.

### 4. Forge feature set — v1 vs. later, mapped to `.px` + mediated events

| Feature | v1? | `.px` procedure(s) | Mediated op / event |
|---|---|---|---|
| Repos (create/list/delete/archive) | ✅ v1 | `repo-lifecycle` | (host/admin ops; nodes) |
| Refs (advertise, CAS-update) | ✅ v1 | `ref-update` (CAS, §3) | `push_received` → `ref.updated` |
| Push (`receive-pack`) | ✅ v1 | `ingest-pack` + `ref-update` | `push_received` |
| Fetch/clone (`upload-pack`) | ✅ v1 | `serve-pack` (negotiation in adapter) | `fetch` |
| Pull requests (open/merge/close) | ✅ v1 | `pr-lifecycle` | `open_pr`, `merge_pr` |
| Issues (open/comment/close) | ✅ v1 | `issue-lifecycle` | `open_issue` |
| Reviews (approve/request-changes) | ✅ v1 | `review-lifecycle` | `create_review` |
| Releases (tag → release) | ⏳ later | `release-lifecycle` | `create_release` |
| Webhooks (register/deliver) | ⏳ later | `webhook-dispatch` | `register_webhook` |
| CI hooks (on push/PR) | ⏳ later | rides `webhook` + ADR-0023 events | (consumes proc events) |
| PR auto-summary/review (LLM) | ⏳ later (optional cap) | `pr-summarize` (via agens) | optional `llm` |

The CRDT-merge nature of PluresDB is a **feature** for the social layer: issue/PR comments
and reviews from multiple participants merge without conflict. Only refs need CAS (§3).
Release and webhook lifecycle are **deferred, not stubbed** (C-NOSTUB-001) — present in the
CID marked `deferred`, with no fake implementation.

### 5. Capabilities — requires / provides

**Requires** (ADR-0024 `[capabilities.required]` / `[dependencies].capabilities`):
- `storage = "^1.0"` (platform, ADR-0011) — PluresDB nodes for refs/metadata; the
  `plures-object` content store is reached through the storage/object substrate.
- `network = "^1.0"` (platform, ADR-0011, `user-approve`) — git transports. **NB: needs the
  streaming-transport extension from §2/Task-3 for production.**
- `secrets = "^1.0"` (provider, via vault per ADR-0024 §3) — deploy keys, push tokens, SSH
  host keys. **Declared dependency**, not a homegrown store.

**Optional:**
- `llm = "^1.0"` (provider, via agens) — PR summarization/review. Feature-detected; absent ⇒
  the summary feature is simply absent (not faked).

**Provides:**
- `git-repo = "1.0.0"` — CID `capabilities/git-repo.cid.toml` (this ADR's File 2).

### 6. UI — forge screens in `ui/` on design-dojo

Per ADR-0024 §5, **product-specific** forge screens live in the plugin's `ui/` (built on
`@plures/design-dojo` primitives), not in design-dojo itself (kit, not catalog):
- **Repo browser** — repo list, file tree, blob view, commit log (reads ref/object nodes).
- **PR view** — PR list, conversation/timeline, **diff** view, review controls, merge button.
- **Issue tracker** — issue list, issue detail + comments.
- **Releases** (when v-later lands).

Long-lived transport operations (a big clone/push in progress) surface through the
**design-dojo side-effect-handler UI** (progress/connection-status), which ADR-0024 §5 places
in design-dojo for exactly these long-lived IO actors.

## Consequences

**Positive**
- A real GitHub alternative whose **HA/DR is inherent** (Hyperswarm P2P object replication +
  read scale-out from any replica), not bolted on. Any `git` client works unmodified
  (smart-HTTP).
- Maximal foundation reuse: objects→`plures-object`, cache→`pares-arca`, sync→PluresDB
  Hyperswarm, secrets→vault. Net-new code is the **git semantics layer + forge `.px`** — the
  irreducible new part — honoring C-PLURES-002/004.
- The social layer (issues/PR comments/reviews) gets **conflict-free multi-writer merge for
  free** from PluresDB CRDT — a genuine advantage over a centralized forge.
- Forge primitives (refs/PR/issue/review/release as reusable `.px`) generalize to other
  project-management plugins (sprint-log, agent-console overlap), per the program's
  reuse mandate.

**Negative / costs**
- The **streaming-transport host capability does not exist yet** — v1 production push/fetch
  is blocked on that Task-3 foundation work (buffered approximation only until then).
- A **per-repo authoritative-writer election** is real distributed-systems machinery (leader
  election over a Hyperswarm topic, failover, read-only degradation window). It must be
  built and tested carefully; it is the highest-risk component.
- Git protocol mechanics (pack negotiation v0/v1/v2, shallow/partial clone, multi-ack) are
  intricate; the adapter is non-trivial even though it composes existing object storage.

**Risks**
- **Ref divergence if CAS/writer-election is wrong** — the single most dangerous failure
  (silent lost pushes / fabricated history). Mitigation: CAS is a CID invariant validated at
  the provider surface; per-repo single-writer serialization; read-only (never divergent)
  degradation when no writer. This is why §3 is decided explicitly and tested first.
- **Pack integrity / object corruption** across P2P replication. Mitigation: content-address
  verification on ingest + a `pack_integrity` CID invariant (object count + checksum
  trailer); `plures-object` already content-addresses, so corruption is detectable by hash.
- **Streaming approximation masking the gap** — shipping the buffered v1 and calling the
  transport "done." Mitigation: the gap is recorded as Task-3 foundation work and the CID
  marks production transport dependencies honestly (C-NOSTUB-001); the buffered path is
  labeled a known limitation, not a finished transport.
- **Auth surface** — deploy keys/tokens are security-critical. Mitigation: depend on
  `secrets@^1.0` (vault); never store credentials in plugin state (no localStorage,
  C-PLURES-003).

## Implementation outline (lifecycle work, gated; design = Pillar 1 only here)

The dev lifecycle (analyze → build → test → deploy → verify) drives each stage as gated
subagents. This ADR is the design (Pillar 1) only.

1. **`git-repo@1.x` CID** (`capabilities/git-repo.cid.toml`, File 2 of this ADR) — the
   contract: node types, mediated ops, events, invariants. Provider validated against it at
   install (ADR-0022 §7).
2. **Object/ref mapping** — `git_object`/`pack` node descriptors referencing `plures-object`
   content; `ref` nodes with `(target_oid, generation)`. Repack maintenance op (`.px`-triggered,
   `git` actor at IO boundary).
3. **Ref-update CAS + per-repo writer election** (§3) — the riskiest piece; **build and test
   FIRST**, in isolation, against the non-fast-forward contract. Hyperswarm topic = repo_id.
4. **Smart-HTTP adapter** — `upload-pack`/`receive-pack` semantics in `adapter/` (TS or Rust
   actor); pack negotiation + integrity verify on ingest.
5. **Streaming-transport host capability (Task 3, foundation/radix)** — general
   long-lived/duplex actor surface; unblocks production push/fetch and `git://`/SSH later.
   Routed to pares-radix (the legitimate side-effect boundary).
6. **Forge `.px` procedures** — repo/ref/PR/issue/review lifecycle over PluresDB nodes;
   release/webhook deferred.
7. **UI** (`ui/` on design-dojo) — repo browser, PR+diff view, issues; side-effect-handler UI
   for long transports.
8. **Tests BLOCK** (provider, ADR-0024 §6 / C-TEST-002): channel-independent — load the plugin
   in a real radix instance, drive `push_received`/`fetch`/`open_pr` via the host, assert
   PluresDB ref/PR state; **build the binary, run a real `git push`/`git clone` against it**
   (C-TEST-001), assert ref CAS rejects a non-fast-forward.

## References
- ADR-0010 — Agens-first plugin model
- ADR-0011 — Plugin security (platform capabilities, mediated IO, trust tiers)
- ADR-0022 — Capability host contract (provider capabilities, CIDs, resolver, binding policy)
- ADR-0023 — Procedure observability event contract (CI hooks consume `plures.proc.event.v1`)
- ADR-0024 — Canonical plugin format (`plugin.toml` + `.px` + adapter + `ui/`; capability deps; `secrets@1.x` via vault)
- `capabilities/commerce.cid.toml` — the reference CID format this CID mirrors
- `capabilities/git-repo.cid.toml` — the `git-repo@1.x` CID authored alongside this ADR
- C-PLURES-002 / C-PLURES-003 / C-PLURES-004 — extend don't reinvent; state in PluresDB; pure logic in PluresDB, IO at the boundary
- C-NOSTUB-001 — no stubs; deferred features marked absent (release/webhook/streaming), never faked
- C-TEST-001 / C-TEST-002 — channel-independent QA; build the binary, run a real `git` client against it
- Foundation primitives: PluresDB (built-in Hyperswarm sync), `plures-object` (content-addressed P2P chunks), `pares-arca` (distributed cache), `plures-vault`→`secrets@1.x`, `agens` (private LLM)
- Program plan: `workspace/memory/plan-radix-plugins-2026-06-26.md` §2.A (thesis + open questions) and §3.B (streaming-transport gap)
