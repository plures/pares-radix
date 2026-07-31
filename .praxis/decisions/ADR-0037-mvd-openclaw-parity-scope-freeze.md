# ADR-0037: MVD (Minimum Viable Dogfood) OpenClaw-parity scope freeze

- Status: Accepted
- Date: 2026-07-24

## Context

`program:pares-radix-platform` pivoted strategy on 2026-07-23: get pares-radix/pares-agens running the home-deploy bootstrap loop as a real dogfood target (`mvd-home-deploy-verification`, DONE; `mvd-dogfood-proof`, in progress) before continuing to chase full feature parity with OpenClaw across every channel and control surface.

At the same time, several `pares-agens` epics that were spun up under the older "build full OpenClaw parity" framing landed real design work in parallel, independent of the pivot:

- `pares-agens:openclaw-parity` — **done**, merged (pares-agens#631, ADR-0018 Discord Spine Channel Adapter design). Known collision: two different design docs both claimed ADR-0018 numbering (SpineChannel/serenity core vs `parity-discord-adapter`'s own ADR-0018); dedup/renumber still needed before either proceeds to implementation.
- `pares-agens:parity-approval-cards` — **done**, merged (pares-agens#632). Design-only PR surfaced a real P0 security gap: `AllowWithApprovalWarning` self-resolves `Allow` without waiting on `pending.wait()`, so no Deny action can actually block a tool call today. Approval wiring is Telegram-only; no channel-agnostic contract, no TTL, no persistence, no audit trail.
- `pares-agens:parity-discord-adapter` — **queued, design done**. ADR-0018-discord-channel-adapter-parity.md (425 lines) confirms zero Discord code exists today (honest absence, not a stub). Blocked on ADR-0017's channel-agnostic core merging first.
- `pares-agens:parity-dispatch-hardening` — **queued, design done**. ADR-0019-dispatch-pipeline-hardening.md found 4 concrete gaps (dead `DeliverySuccess`/`DeliveryFailure` variants, no shared retry/backoff, uneven single-in-flight enforcement across channels, Tauri stuck on the legacy adapter path). Not yet committed/PR'd.
- `pares-agens:autonomous-milestone-reporting-reliability` — **in progress**. ADR-0020 found completion reporting is non-durable (in-memory only), `kill()` creates no `CompletionEvent`, and `spawn_completion_forwarder` is dead code with no production call site.
- `pares-agens:skill-md-loading` — **done**, PR #658 (feat/parity-skill-discovery), needs rebase + review.

None of this design work is wasted, but none of it is on the bootstrap critical path either, and continuing to actively dispatch it competes for the same limited execution slots as `mvd-dogfood-proof` and `plures-backlog-handoff-plan`. A scope decision is needed so in-flight epics are correctly triaged instead of silently starving the bootstrap loop of priority.

## Decision

**MVD (Minimum Viable Dogfood) scope is: prove pares-radix/pares-agens can autonomously receive, dispatch, execute, and report on real backlog tasks end-to-end on a home-deployed target, using the channels and controls that already exist and already work.** Full OpenClaw feature parity is explicitly deferred past MVD.

### IN scope for MVD

1. **Home-deploy verification and the autonomous dispatch/execution loop** (`mvd-home-deploy-verification` — done; `mvd-dogfood-proof` — in progress). This is the actual bootstrap critical path.
2. **Existing, already-wired channel(s)** — whatever channel MVD dogfooding already runs over (Telegram/stdio/http_spine as currently implemented). No new channel adapters are required for MVD.
3. **Durable task custody** (ADR-0036) and **task-dispatch verb resolution** (ADR-0034) — already landed, load-bearing for the loop, kept as-is.
4. **The P0 approval-enforcement bug specifically** — `AllowWithApprovalWarning` proceeding without waiting on `pending.wait()` is a correctness/safety defect in code that ships today, not a parity feature. It stays in scope as a bug fix, tracked separately from the broader `parity-approval-cards` design (channel-agnostic `ApprovalCard` contract, TTL/expiry, persistence, audit trail — those are OUT, see below). Fix the wait-on-pending gap; do not build the rest of the approval framework yet.
5. **skill-md-loading** (`pares-agens:skill-md-loading`) — already done and low-risk to land (PR #658, needs rebase + review only). Finishing an already-merged-adjacent PR is cheaper than leaving it to rot; land it, don't reopen scope.

### OUT of MVD scope (explicitly deferred)

1. **Discord channel adapter** (`pares-agens:parity-discord-adapter`) — zero code exists; design (ADR-0018-discord-channel-adapter-parity.md) is complete and can wait. Not needed to prove the dogfood loop.
2. **Dispatch pipeline hardening beyond the P0 approval-wait fix** (`pares-agens:parity-dispatch-hardening`, ADR-0019) — retry/backoff unification, delivery ack events, per-channel in-flight enforcement, Tauri SpineChannel migration. Real gaps, but MVD does not need every channel hardened uniformly; the dogfood channel already works well enough to test with.
3. **Full approval-card framework** (`pares-agens:parity-approval-cards` beyond the P0 fix) — channel-agnostic `ApprovalCard` contract, TTL/expiry, cross-restart persistence, idempotent-press UX, audit trail. Needed for multi-channel parity, not for proving the loop on one channel.
4. **Mobile/Flatpak installers** (ADR-0030-mobile-tauri-targets.md territory) — packaging and distribution are irrelevant until there is a proven dogfood loop to distribute.
5. **Hyperswarm git forge integration** (ADR-0025-hyperswarm-git-forge.md) — decentralized forge sync is a scaling/resilience feature for a multi-node future, not a bootstrap-loop requirement.
6. **Sandbox / capability-host hardening beyond what's already shipped** (ADR-0022-capability-host-contract.md territory) — deepening isolation guarantees is a hardening pass for when more surface area is exposed (Discord, mobile, multi-user), which is itself out of scope.
7. **Durable/replayable milestone-reporting pipeline** (`pares-agens:autonomous-milestone-reporting-reliability`, ADR-0020) — the *current* in-memory reporting is good enough to observe MVD dogfood test results manually; the durable outbox/retry/replay pipeline is a reliability upgrade for when reporting must survive process restarts unattended, which is a post-MVD concern. The design work is not thrown away — it becomes the first item after MVD proves out.
8. **ADR-0018 numbering collision resolution** (between `openclaw-parity`'s Discord ADR and `parity-discord-adapter`'s Discord ADR) — deferred along with Discord itself; no need to renumber a design doc for a feature that isn't being implemented yet.

## Consequences

- `mvd-dogfood-proof` and `plures-backlog-handoff-plan` are the only P1/P2 pares-radix-platform-lineage epics that should be actively dispatched next; the seven items above move to a deferred/backlog state rather than continuing to consume dispatch slots.
- The completed design docs (ADR-0018 Discord ×2, ADR-0019 dispatch hardening, ADR-0020 milestone reporting) remain valid and are not re-litigated — they are simply queued behind MVD proof instead of raced against it.
- The P0 approval-wait bug is tracked and fixed independent of the broader `parity-approval-cards` epic, since it is a safety defect in shipped code, not a scope question.
- `pares-radix:plures-backlog-handoff-plan` is now unblocked to define which epics (including the eight OUT-of-scope items above) move from OpenClaw-orchestrated dispatch to radix-at-home ownership, since this ADR gives it a concrete, defensible IN/OUT list to hand off against.
- Any future work item that reopens Discord, Hyperswarm, mobile/Flatpak, sandbox hardening, full approval-card framework, or durable milestone-reporting before MVD is proven should cite this ADR and be re-triaged, not silently resumed.
