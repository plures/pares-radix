# commerce — capability provider plugin

**Satisfies CID:** `commerce@1.x` (`capabilities/commerce.cid.toml`, the org-owned
Capability Interface Descriptor).
**Provides:** `commerce = "1.0.0"` → satisfies the consumer requirement
`commerce = "^1.0"` declared by `inner-space`.
**ADRs:** ADR-0022 (Capability Host Contract, step 4 = the provider plugin) +
ADR-0011 (mediated cross-plugin).

## What this is

The **anonymous verifiable commerce** provider: issue cryptographic ZK coupons
against a campaign, authorize/redeem them with zero-knowledge proofs, and prevent
double-spend via a **grow-only nullifier set**. Tiered confirmation
(instant micro / delayed large) is a deployment **policy**
(`TierPolicy.largeThreshold`), not a fixed number.

This plugin ports the [OASIS G2 ZK-commerce logic](../../../oasis/packages/crypto-verification)
**behind** the `commerce@1.x` contract. **It reimplements no cryptography**
(C-NOSTUB-001 / no-reinvention): the coupon commitments, the eligibility proofs,
the nullifier derivation, and the PluresDB-backed grow-only set are the
**shipped, real** `@oasis/crypto-verification` code path. PluresDB is the source
of truth for the nullifier set (C-PLURES-003).

## Where the code lives (declaration here, actor in OASIS)

```
pares-radix/plugins/commerce/            ← the Radix plugin DECLARATION
  plugin.toml                            ← manifest: provides commerce@1.0.0,
                                           schema (5 CID node types), logic procs
  praxis/commerce-mediation.px           ← .px: the mediated request→actor→result flow
  praxis/redemption-tier.px              ← .px: the pure tier rule (twin of decideTier)
  adapter/commerce-provider.ts           ← pointer → the real OASIS actor (no dup copy)

oasis/packages/crypto-verification/      ← the REAL logic this plugin adapts
  src/commerce-provider/index.ts         ← CommerceProvider: the side-effecting IO actor
                                           (wraps RedemptionProtocol / CouponService /
                                            PluresDbNullifierStore / decideTier)
  src/__tests__/commerce-provider-mediated.test.ts  ← the mediated e2e
```

The IO actor (`CommerceProvider`) lives in the OASIS package — **not** copied
into this plugin dir — because it IS OASIS commerce logic and must typecheck +
test against the real protocol/nullifier surface. A second compilable copy in the
radix tree could drift from the real OASIS surface, which would violate the
no-stub/no-reimplementation gate. The plugin dir holds the **declaration** the
loader validates; `adapter/commerce-provider.ts` documents the pointer.

## How the mediation works (no direct calls cross the boundary)

The only thing crossing the plugin boundary is **PluresDB state** (ADR-0011):

```
 consumer (inner-space)                            provider (CommerceProvider)
 ──────────────────────                            ───────────────────────────
 putRequest(commerce:request:<op>:<id>, payload) ─▶ PluresDB
                                                     └─ provider.react(op, id):
                                                          getValue(request) →
                                                          REAL OASIS actor →
                                                          put(commerce:result:<op>:<id>)
                                                          (+ grow nullifier set)
 readResult(commerce:result:<op>:<id>)           ◀─ PluresDB
```

1. The consumer **writes a request node** (`commerce:request:<op>:<id>`) — it
   holds NO reference to `CommerceProvider` or to any OASIS symbol.
2. The host event bus invokes `provider.react(op, requestId)` on the
   `commerce.<op>.requested` event — the reactive body the `.px`
   `on_<op>_requested` procedures describe.
3. `react` reads the request node, calls the **real**
   `@oasis/crypto-verification` `RedemptionProtocol` / `decideTier` at the IO
   boundary, and **writes the result node** (`commerce:result:<op>:<id>`), growing
   the PluresDB nullifier set on redemption.
4. The consumer **reads the result node**. `putRequest` / `readResult` are the
   consumer-side helpers — they touch ONLY the KV.

## Operations (1:1 with the CID + the real OASIS surface)

| operation | request event | result event | real OASIS call |
|---|---|---|---|
| `issue_coupon` | `commerce.issue.requested` | `commerce.issue.completed` | `RedemptionProtocol.issueCoupon(campaignId, value)` |
| `authorize_redemption` | `commerce.redeem.requested` | `commerce.redeem.completed` | `RedemptionProtocol.authorizeRedemption({coupon, amount, policy})` → `E004` on double-spend |
| `check_nullifier` | `commerce.nullifier.check.requested` | `commerce.nullifier.check.completed` | `createNullifierRegistry(store, campaignScope).isSpent(nullifier)` (read-only) |
| `decide_tier` | `commerce.tier.decide.requested` | `commerce.tier.decide.completed` | `decideTier(amount, policy)` — boundary **inclusive** of micro |

> Scope note: the nullifier set is **per-campaign** (the redemption path scopes the
> registry by the coupon's `campaignId`). A `check_nullifier` request therefore
> carries the `campaign_id` so the membership read targets the SAME scope the
> redemption claimed under. (Fixed during slice 4b — see the report.)

## Invariants upheld (CID `[invariants]`)

- **double_spend_prevented** — a nullifier already in the set ⇒
  `authorize_redemption` fails with `E004`.
- **nullifier_set_grow_only** — the OASIS `PluresDbNullifierStore` has no remove
  path (union-write only).
- **tier_boundary_inclusive** — `instant` iff `amount <= largeThreshold`.
- **status_monotonic** — a redeemed coupon never returns to active (re-redeem is
  `E004`).
- **anonymous_redemption** — the result node carries no identity/opening; the
  secret coupon opening is holder-side and never appears on a result node.

## Testing

The mediated end-to-end test
(`oasis/packages/crypto-verification/src/__tests__/commerce-provider-mediated.test.ts`)
drives the provider THROUGH THE MEDIATED REQUEST/RESULT NODE SURFACE only — the
consumer side touches nothing but the PluresDB KV (`putRequest` / `readResult`)
and never calls the OASIS logic directly. It proves: `issue_coupon` →
`authorize_redemption` (authorized, instant) → second `authorize_redemption` of
the same coupon (`E004`, double_spend_prevented); `decide_tier` inclusive
boundary (99/100 → instant, 101 → delayed; default policy 0/1); `check_nullifier`
membership both ways; and that **every key crossing the boundary** is a
`commerce:request:*` / `commerce:result:*` / `oasis:nullifier:set:*` node.

Per C-TEST-002 the test depends on **no channel adapter**; the only substituted
seam is the PluresDB KV storage transport (an in-memory `PluresKvClient` double
using `structuredClone` to model the serialized node boundary). Production
authority stays PluresDB via the real `PluresDbNullifierStore` (C-PLURES-003).
It lives in the OASIS workspace because the actor it exercises is OASIS code; it
runs in the package's default `pnpm test` (resolves the real crypto via relative
imports, no alias needed).
