# praxis/shadow — Umbra-evolved shadow candidates (INERT)

This directory holds **shadow classifiers** evolved by [`pares-umbra`](https://github.com/plures/pares-umbra)
(the evolutionary arena). They are deployed **inert** and exist to be evaluated against real traffic
*in the shadow* — they **never affect live output** until they consistently beat the live classifier.

## What these are

| File | Evolved model | Reported fitness |
|------|---------------|------------------|
| `shadow_route_message.px`  | `route_message`  | Accuracy 97.5% (routes: code_review, bug_report, deploy, monitoring, chat) |
| `shadow_score_priority.px` | `score_priority` | inv-MSE 0.2960 (priority 0–10) |
| `shadow_classify_intent.px`| `classify_intent`| Accuracy 95.8% (question, command, statement) |

Each file has two parts:

1. **A loadable, manual-trigger pares-radix procedure** named `shadow_<model>` (e.g. `shadow_route_message`).
   It declares `trigger: manual`, so `bootstrap::register_reactive_procedures` **skips it** — it is *not*
   registered on `inbound:*` or any live trigger pattern. Its body routes through the umbra-backed
   `evaluate_shadow_classifier` action, carrying the model id.
2. **An `EVOLVED-SOURCE` provenance block** at the bottom, containing the *verbatim* evolved `.px` source
   (weights, network structure, accuracy header), each line `//`-prefixed.

## Why the evolved source is embedded as a comment (not the live body)

pares-radix's `.px` engine (`pluresdb-px`) does **not** parse umbra's brace-block dialect:

```
procedure route_message { facts { f0: number ... } let hidden_0 = input.f0 * 2.24  when X>Y then emit "code_review" }
```

The pluresdb-px grammar parses `procedure name { ... }` as a Rust-style `code_block` that requires
semicolons and `emit(...)` call syntax; umbra's `facts {…}`, bare `let …`, and `when…then emit "…"`
are **not** valid there and produce a hard parse error. (Verified empirically — see
`crates/core/tests/shadow_inert.rs` and the dev note `memory/2026-06-17-umbra-shadow-deploy.md` in the
agent workspace.) Block comments (`/* */`) are also unsupported by the grammar, so the provenance lines
are each `//`-prefixed.

Keeping the verbatim source here means:
- The exact evolved network is preserved for re-evolution / promotion.
- The file still **loads** as a valid pares-radix procedure (non-empty, manual, inert).
- We do **not** smuggle an unparseable file into the tree (which would be silently skipped — a bandaid).

## How they ride to praxisbot

`flake.nix` `postInstall` copies the **entire** `praxis/` tree into the package
(`cp -r ./praxis $out/share/pares-radix/praxis`), and the praxisbot NixOS service syncs
`$pkg_share/praxis → ~/praxis` on every start. So `praxis/shadow/` deploys automatically with the
normal autoUpgrade path — **no flake change required**.

## How they accumulate fitness (and eventually promote)

At startup the CLI loads `praxis/shadow/*.px` into a **shadow holder** (`ShadowProcedures`,
`pares_agens_core::spine::shadow`), *separate from and after* the live `ReactiveRegistry` registration.
The holder never registers shadow procedures reactively; it just makes the candidates available for
out-of-band evaluation.

The **evaluation/evolution loop stays in umbra** (per the `C-PLURES` constraint — pares-radix does not
host a second evolutionary engine). The eventual flow:

1. Shadow holder exposes the loaded candidates + their model ids.
2. An umbra-shadow arena (future wiring) scores each candidate against the same real traffic the live
   classifier sees, accumulating fitness.
3. Only when a candidate **consistently** beats the live classifier is it promoted into the live
   routing path (a separate, deliberate step — not done here).

## Regenerating these files

`_generate_shadow.ps1` regenerates all three from `pares-umbra/data/*.px`:

```powershell
pwsh ./praxis/shadow/_generate_shadow.ps1
```

It re-reads the latest evolved source, re-prefixes the provenance block, and rewrites the
manual-trigger wrappers. Run it after re-evolving in umbra.

## Invariants (enforced by tests)

- `shadow_*` procedures register **0** reactive triggers (`crates/core/tests/shadow_inert.rs`).
- They do **not** collide with the live dataflow `procedure route_message(...)` in
  `praxis/procedures/routing.px` (different names).
- Loading the real `praxis/shadow/` dir yields exactly the 3 candidates, all `manual`.
