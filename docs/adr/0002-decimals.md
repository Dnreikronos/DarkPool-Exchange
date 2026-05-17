# ADR 0002 — Decimals contract across wire, ZK, and on-chain

- **Status:** Accepted
- **Date:** 2026-05-16
- **Tag:** C9
- **Issue:** [#89](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/89)

## Context

A single trade amount is rendered, validated, transmitted, proved, and
settled by four subsystems that do not agree on what a number looks
like:

| Boundary                          | Encoding                                  | Source                                            |
|-----------------------------------|-------------------------------------------|---------------------------------------------------|
| User input / display              | Free-form decimal text                    | UI                                                |
| REST / gRPC wire                  | Decimal **strings** (`"3000.50"`)         | `crates/dp-api/proto/darkpool/v1/darkpool.proto`  |
| ZK encoder (proof field elements) | `i128`, multiplied by **1e8**, hard cap `< 2^60` | `crates/dp-zk/src/encoding.rs`             |
| On-chain settlement               | `uint256` in ERC-20 base units            | `contracts/src/DarkPool.sol`                      |

The proto declares `string price`, `string size`, `string clearing_price`,
`string matched_volume`. The contract's `_settleMatch` computes
`notional = m.price * m.size / 1e18`. The ZK encoder rejects any
decimal that does not fit at 8 decimal places. Three independent
constraints, one user value — without a single source of truth this
will silently break in subtle ways (rounding drift, lost dust,
off-by-decimals settlement, proof rejections at submit time).

This ADR fixes the contract so every consumer treats numbers the same
way and gives the frontend a single module — `front/lib/units.ts` —
that owns every conversion.

## Decision

### 1. Token decimals are pinned per pair

Pair set is hard-coded for the MVP. Multi-pair waits on backend
issue [#29](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/29).

| Pair        | Base  | Base decimals | Quote | Quote decimals |
|-------------|-------|---------------|-------|----------------|
| `ETH/USDC`  | WETH  | 18            | USDC  | 6              |

These numbers come from the ERC-20s themselves and are immutable. The
frontend MUST source them from a typed table in `units.ts`, never from
runtime contract reads.

### 2. The wire format is a decimal string, capped at 8 dp

All numeric fields on the REST and gRPC wire (`price`, `size`,
`remaining_size`, `clearing_price`, `matched_volume`, `total_size`)
are serialized as decimal strings with up to **8 fractional digits**.
The 8 dp ceiling is not arbitrary — it is the precision the ZK
encoder accepts (`DECIMAL_SCALE = 8`). Anything finer would prove and
settle differently from what was submitted.

Canonical form:

- No leading zeros (`"3000"`, not `"03000"`).
- No trailing zeros after the decimal point (`"3000.5"`, not
  `"3000.50"`; `"3000"`, not `"3000."`).
- No exponential notation (`"1000"`, not `"1e3"`).
- No sign — negatives are rejected upstream of the wire.

This is the form `decimal.js`'s `.toString()` emits, so the frontend
gets canonicalization for free as long as it routes through
`decimal.js`.

### 3. The "1e8" wire format mentioned in the engine is internal to the prover

The `1e8` scale in `crates/dp-zk/src/encoding.rs` is not a wire format
the frontend ever sees. It is the prover's choice of field-element
encoding, applied **inside** the WASM prover (issues
[#97](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/97),
[#98](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/98))
before the proof is emitted. The frontend feeds the prover the same
`Decimal` it uses everywhere; the prover handles the 1e8 scaling and
the `2^60` range check.

The wire-level invariant the frontend MUST enforce — and that
`units.ts` enforces — is the **8 dp ceiling** and the
`scaled < 2^60` ceiling (≈ raw value `< 1.15e10`). Submitting a value
that violates either of these is guaranteed to fail proof generation
later, so we reject it at the form layer.

### 4. On-chain math: size in base decimals, price in quote decimals

The `_settleMatch` formula is

```
notional = m.price * m.size / 1e18
```

with `m.size` and `m.price` as `uint256`. For this to produce a
notional denominated in quote-token base units, the scaling must be:

- `m.size = size_decimal × 10^base_decimals` (WETH wei, `× 1e18`).
- `m.price = price_decimal × 10^quote_decimals` (USDC base, `× 1e6`
  per whole WETH).

That is, the on-chain `price` is **not** scaled by `1e18`. It is
scaled by the **quote** token's decimals. The `1e18` divisor in the
contract is the base token's decimal divisor (it cancels with the
`size` scaling), not a global constant.

| Field          | Scale                       | For ETH/USDC     |
|----------------|-----------------------------|------------------|
| `m.size`       | `× 10^base_decimals`        | `× 1e18` (WETH)  |
| `m.price`      | `× 10^quote_decimals`       | `× 1e6` (USDC)   |
| `m.price * m.size / 1e18` | quote base units | USDC base units  |

`units.ts` exposes the generic helpers required by the issue —
`toOnchainAmount(value, decimals)` / `fromOnchainAmount(value, decimals)`
— and the caller chooses the right `decimals` for the field it is
encoding (size → base, price → quote). The ADR is the place that
records *which decimals for which field*; the helpers themselves are
deliberately field-agnostic so the same primitive works for deposits,
withdrawals, balances, and fee math.

### 5. The frontend type contract

| Value role                     | Type                          |
|--------------------------------|-------------------------------|
| Internal math, validation      | `Decimal` (from `decimal.js`) |
| API wire I/O, display          | `string`                      |
| On-chain calldata / event data | `bigint`                      |

`number` is **forbidden** for any traded quantity. The only `number`
allowed in this domain is the `decimals` scalar itself (an integer in
`[0, 18]`) and array indices.

Helpers that accept a "decimal" input take `Decimal | string`,
never `number`. Coercion to `number` anywhere in the read or write
path is a bug; lint rules around `units.ts` and form components
should treat `Number(...)` over a price/size as a code-review red
flag.

### 6. Rounding policy

- **Protocol paths (wire, prover, on-chain):** no rounding. If a
  value carries > 8 dp at the wire boundary, or > `base_decimals` /
  `quote_decimals` at the on-chain boundary, `units.ts` throws. There
  is no "round down to fit" — silent rounding here is what loses
  user funds.
- **Display:** round-half-up to a per-field display precision.
  Defaults for ETH/USDC: price → 2 dp, size → 4 dp. Higher precision
  is available for power-user views; the round is for the formatter
  only and never feeds back into a submitted value.

`Decimal.ROUND_HALF_UP` is the default rounding mode for display.
Display rounding never changes the underlying `Decimal` — it
produces a new display string.

### 7. `front/lib/units.ts` is the only entry point

Once this lands, anywhere in `front/` that does `Number(price)`,
`parseFloat(size)`, or hand-rolled `× 10^n` is a bug. `units.ts`
re-exports the `Decimal` constructor so callers do not need to
import `decimal.js` directly, which keeps the boundary tight and
makes audit greppable.

API surface:

```ts
// Parsing / normalization
toDecimal(value: Decimal | string): Decimal

// Wire ↔ Decimal
toWireSize(value: Decimal | string): string
toWirePrice(value: Decimal | string): string
fromWireSize(wire: string): Decimal
fromWirePrice(wire: string): Decimal

// On-chain ↔ Decimal
toOnchainAmount(value: Decimal | string, decimals: number): bigint
fromOnchainAmount(value: bigint, decimals: number): Decimal

// Display
formatPrice(value: Decimal | string, displayDp?: number): string
formatSize(value: Decimal | string, displayDp?: number): string

// Pinned per ADR §1
export const TOKEN_DECIMALS: { WETH: 18; USDC: 6 }
export const WIRE_MAX_DP = 8
export const WIRE_MAX_SCALED = 2n ** 60n
```

`viem.parseUnits` / `viem.formatUnits` are used internally by the
on-chain helpers. They are not re-exported — callers go through
`toOnchainAmount` / `fromOnchainAmount` so the ADR's invariants are
enforced in one place.

## Consequences

- A single regression test suite (`front/lib/units.test.ts`) protects
  every numeric boundary in the frontend.
- Form validation has a concrete rule: "must parse, must be ≤ 8 dp,
  must not exceed `WIRE_MAX_SCALED`". UX copy can be written against
  exactly those failure modes ([#92](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/92), [#99](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/99)).
- The future WASM prover ([#97](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/97), [#98](https://github.com/Dnreikronos/the-DarkPool-Exchange/issues/98))
  accepts `Decimal`/string and is responsible for the 1e8 + 2^60
  scaling on its side. The frontend does not pre-scale.
- Adding a pair means appending to `TOKEN_DECIMALS` and updating
  display defaults; no math helper changes.
- The 8 dp wire ceiling is a real constraint on smallest tradable
  size: `1e-8` WETH ≈ `0.00000001` ETH. Comfortable for the MVP.

## Alternatives considered

- **Scale everything by `1e18` on the wire.** Mirrors the contract,
  but throws away precision the proto explicitly preserves (strings),
  forces every consumer to know the scale, and makes the engine's
  1e8 ZK boundary invisible — exactly the failure mode this ADR is
  written to prevent.
- **Use JS `number` with a 53-bit-safe limit.** `Number.MAX_SAFE_INTEGER`
  has no relationship to BN254 field size or to ERC-20 decimals;
  picking it would conflate display ergonomics with protocol safety
  and would still drop precision below 8 dp for large notionals.
- **Drop `decimal.js` and use only `bigint`.** Workable but ugly:
  every price math op would need an explicit scale, and display
  rounding becomes hand-rolled. `decimal.js` is ~30 kB and earns its
  weight on every form.
