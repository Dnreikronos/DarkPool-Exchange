# ADR 0005 — Matching priority and fill allocation

- **Status:** Accepted
- **Date:** 2026-06-01
- **Issue:** [#161](https://github.com/Dnreikronos/DarkPool-Exchange/issues/161)

## Context

Each auction round clears at a single uniform price. Once that price is
fixed, the engine must decide **which** of the eligible orders get filled,
and in what order, when one side is oversubscribed at the clearing price.
Before this ADR that decision was implicit and unsafe:

1. `dp_auction::run` sorted `bids` by `Reverse(price)` and `asks` by
   `price` **only** — no tiebreak. It leaned on the caller passing
   pre-sorted slices and on `sort_by_key` being stable. The order book
   supplied a `submitted_at` tiebreak, but that is `Utc::now()` captured at
   placement with no sequence fallback, so two orders in the same
   millisecond had an ambiguous order.
2. On recovery, `submitted_at` is reconstructed from the event timestamp
   (`recover.rs`), which for the Postgres store is the **DB write time**,
   not the original placement instant. Replayed priority could therefore
   differ from live priority — the same event log could clear to a
   different matching than was settled.
3. `match_orders` is greedy: the first eligible order on each side consumes
   the opposing side in sort order until exhausted. With a non-deterministic
   sort that meant a marginal sub-millisecond timing difference could decide
   who captured all the liquidity, with no documented rule.

There was also no explicit check that volume is conserved leg-for-leg
(`Σ bid fills == Σ ask fills`) or that every fill prices at the clearing
price. Both held implicitly but were never asserted.

## Decision

### 1. Priority rule: strict price-time

Within a round, orders are matched by **strict price-time priority**:

- better price first (higher bid / lower ask), then
- earlier placement first.

We do **not** use pro-rata allocation. The highest-priority order on each
side is filled as far as the opposing liquidity allows before the next is
considered. The marginal order at the clearing price may be partially
filled; lower-priority orders behind it get nothing that round.

Rationale: price-time is the well-understood default for a continuous book,
keeps the matcher simple and exact (no fractional pro-rata remainder to
round, which matters because sizes are `Decimal` and on-chain settlement is
integer base units — see [ADR 0002](0002-decimals.md)), and preserves the
existing semantics rather than changing market behaviour while fixing a
correctness bug. Pro-rata remains a possible future change; it would need
its own ADR and a defined remainder-allocation rule.

### 2. Priority key: a monotonic `seq`, not `submitted_at`

The total order is `(price, seq)`, where `seq` is the **event-store
sequence number** stamped on the order's `OrderPlaced` event when it is
appended. `dp_types::Order` carries this as a new `seq: u64` field.

`seq` is the right key because it is:

- **unique** — strictly increasing across every order, so `(price, seq)` is
  a strict total order with no ambiguity even at sub-millisecond placement;
- **replay-stable** — on recovery the order is rebuilt from its
  `OrderPlaced` event and takes that event's own `seq`, the identical value
  the live path stamped. Live and replayed books therefore agree on
  matching order regardless of when the DB happened to write the row.

`submitted_at` is retained as a wall-clock instant for display and TTL only.
It must never drive matching priority.

### 3. Ordering is established inside `run`

`dp_auction::run` now sorts its inputs by `(price, seq)` itself, instead of
trusting the caller. The matching result is a pure function of the order set
and is independent of the slice order handed in. `OrderBook::bids` / `asks`
apply the same `(price, seq)` tiebreak so that observers of the book see the
same priority the matcher uses, but the matcher no longer depends on that.

### 4. Conservation is asserted

`match_orders` debit-and-credits each match by an identical `fill_size` at
the clearing price, so `Σ bid fills == Σ ask fills` and every fill prices at
the clearing price by construction. These invariants are now `debug_assert`-ed
in the matcher (no release-build cost) and covered by unit tests in
`dp-auction`.

## Consequences

- The same event log always replays to the matching that was settled.
  Recovery is deterministic with respect to fill allocation.
- Priority is unambiguous at any placement granularity; there is no
  sub-millisecond race for liquidity.
- Adding a field to `Order` changes its serialized shape. The book snapshot
  is bincode (positional, not self-describing), so `#[serde(default)]` does
  **not** make a pre-`seq` snapshot decode; it only covers self-describing
  formats such as JSON. A stale snapshot fails to decode, the recover path
  treats that envelope as corrupt and tries older envelopes, and ultimately
  falls back to a full event replay, which reconstructs each order's real
  `seq` from its `OrderPlaced` event. The one gap: if events were compacted
  past the snapshot point (`compact_before`), full replay can no longer
  rebuild the book and recovery fails. This is pre-prod with no snapshots in
  existence yet, but the wire-shape change is real and snapshot compatibility
  rests on the replay fallback, not on `serde(default)`. `seq` is **not**
  part of the ZK commitment (derived from id / key / side / price / size), so
  proofs are unaffected.
- The marginal-time advantage at the clearing price is now deliberate and
  documented (price-time), not an accident of timestamp resolution. If the
  protocol later wants to remove it, pro-rata is the lever — a separate ADR.

## Alternatives considered

- **Keep `submitted_at` but add a sequence fallback to it.** Still leaves a
  wall-clock value in the priority key and a second source of truth; `seq`
  alone is simpler and already monotonic.
- **Pro-rata allocation at the clearing price.** Fairer for size and more in
  the spirit of a batch auction (no within-batch time advantage), but a
  larger change: it needs a defined, deterministic rule for the indivisible
  `Decimal` remainder, and it changes market behaviour. Deferred; would get
  its own ADR.
- **Sort only in the caller (the book) and keep `run` trusting it.** This is
  the status quo that broke. Establishing the order inside `run` makes the
  matcher correct in isolation and testable without a book.
