# Expectation: NO STUBS — anywhere, ever (C-NOSTUB-001)

## Status: Enforced
## Date: 2026-06-23
## Origin: kbristol directive (Telegram #33973). Hard policy.

## Constraint ID: C-NOSTUB-001

Do it right or don't do it at all. No stub, placeholder, mock, fake, dummy, or
"wire it up later" in any code path meant to be functional. If a thing is not really
implemented, it is not declared, not registered, not exported, not advertised as present,
and never reported as done.

## Rationale

Stubs are invisible cross-session debt. This agent has a demonstrated history of losing
track of what is stubbed vs. real, then reporting hollow work as complete and shipping
non-functional features. Therefore stubs are banned outright rather than tracked.
Honest absence > dishonest completeness.

## What violates this constraint

- `todo!()`, `unimplemented!()`, `panic!("not implemented")`, `unreachable!()` standing
  in for real logic.
- A function returning a hardcoded/canned value instead of computing the real result.
- Empty bodies / `return Ok(())` / `return true` that skip the real work while claiming
  to do it.
- Identifiers or comments signalling a stub in functional code: `stub`, `placeholder`,
  `mock`, `fake`, `dummy`, `for now`, `TODO: implement`, `wire up later`, `FIXME: stub`.
- A provider/adapter/capability registered as satisfying an interface (e.g. ADR-0022
  `[capabilities.provided]`) without a real implementation behind it.
- Runtime/UI bound to fabricated data instead of the real store (overlaps "Demos & Data"
  and C-PLURES-003).

## What is allowed (NOT a stub)

1. **Genuine absence** — the feature is not declared/registered/exported/in-manifest.
   It simply does not exist yet. This is the correct way to ship "not done."
2. **Real narrow error** — a genuinely-unavailable capability returns a real
   `CapabilityUnavailable`-style error the caller handles; the feature is not advertised
   as present. (Feature detection, per ADR-0022 §3 optional-capability handling.)
3. **Documented unit-test double** — a mock used only to isolate a dependency inside a
   unit test, never in a shipped runtime path, never as the thing under test
   (consistent with C-TEST-002).

## Enforcement

- **Static gate (pre-push / CI):** reject functional source containing the banned forms
  above. Allowed exceptions must be inside `#[cfg(test)]` / test files and explicitly
  annotated. (To be wired into the repo's pre-push check alongside the praxis-duplication
  check.)
- **Runtime gate (ADR-0022):** a plugin declaring `[capabilities.provided]` must pass the
  CID surface validation against a real implementation; a provider that cannot really
  service the CID must not register.
- **Behavioral gate (AGENTS.md):** if real implementation is not possible this turn,
  report "not built" and leave it absent — never fill the hole with a fake.

## Interaction with ADR-0022 (Capability Host Contract)

This kills the "stub scene/physics first" shortcut. Under C-NOSTUB-001, the first
inner-space run binds only capabilities that have **real** providers. Capabilities without
a real provider are simply **unsatisfied** (consumer required → activation blocked with an
actionable error; consumer optional → feature-detected absent). No stub provider is ever
registered to make activation "succeed" hollowly.

## References
- AGENTS.md "HARD GATE: NO STUBS — ANYWHERE, EVER"
- MEMORY.md C-NOSTUB-001
- ADR-0022 §1, §3 (provider capabilities, optional/unsatisfied handling)
- C-TEST-002 (channel-independent QA; test doubles only at real seams)
- C-PLURES-003 (state through PluresDB, no fabricated/ad-hoc state)
- SOUL.md "No more bandaids" / "Never ship hope"
