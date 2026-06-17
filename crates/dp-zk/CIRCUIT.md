# DarkPool Batch-Proof Circuit

`dp-zk::circuit::BatchProofCircuit` is a Groth16 circuit over BN254 that
proves the validity of a fixed-size batch of matched-pair fills without
revealing per-trader identities, balances, or pre-trade positions.

## Public inputs

The verifier checks the proof against six BN254 scalars, in this order:

| # | Name               | Meaning                                                |
|---|--------------------|--------------------------------------------------------|
| 1 | `match_count`      | Number of active (non-padded) match rows.              |
| 2 | `commitments_root` | Poseidon root over all `2 * batch_size` leg commits.   |
| 3 | `notionals_root`   | Poseidon root over `price_i * size_i` per match.       |
| 4 | `min_size`         | Policy: minimum allowed fill size (1e-8 unit).         |
| 5 | `min_price`        | Policy: minimum allowed clearing price (1e-8 unit).    |
| 6 | `position_limit`   | Policy: signed position cap (`±2^58` by default).      |

The `position_limit` is encoded via `signed_to_scalar`: positive integers
go in the canonical range, negatives via `Fr - x`. The default policy
uses `2^58`.

## Constraint families (v4)

| # | Family                      | Active-only? | Mechanism                                           |
|---|-----------------------------|--------------|-----------------------------------------------------|
| 1 | Side bit-form               | All rows     | `side * (1 - side) == 0` per leg.                   |
| 2 | Opposite sides              | Active rows  | `(bid_side + ask_side - 1) * is_active == 0`.       |
| 3 | Crossing                    | Active rows  | `bid_lp - match_price` and `match_price - ask_lp` non-negative via 60-bit range. |
| 3b| Uniform clearing price      | Active rows  | `(match_price - clearing_price) * is_active == 0` — every active row settles at the single auction clearing price (#163). This proves uniformity only; it does not prove the price maximizes matched volume. |
| 4 | Min-size, min-price         | All / active | 60-bit ranges on `size`, `price`; diff to floors gated by `is_active`. |
| 5 | Leg commitment binding      | All rows     | In-circuit Poseidon over `(trader_id, side, lp, size, salt)` reconstructs the commitment, accumulated into `commitments_root`. |
| 5b| Input-completeness membership (IVC step circuit, #157) | Active rows | Each active leg's reconstructed commitment is proven a leaf of the round's admitted-set Merkle root: `(merkle_root(leg_commit, path) - admitted_root) * is_active == 0`. Ties every settled order to the publicly admitted input set. |
| 6 | Notional binding            | All rows     | `match_price * match_size` accumulates into `notionals_root`. |
| 7 | Solvency                    | Active rows  | `balance < 2^60` (60-bit range) AND `balance * 1e8 - notional` fits 120-bit range. |
| 8 | Position-limit (two-sided)  | Active rows  | For `new_pos = position ± match_size`, `(limit - new_pos)` and `(limit + new_pos)` each fit in 60 bits → `\|new_pos\| ≤ position_limit`. |
| 9 | Trader-id binding           | Active rows  | `(poseidon(trader_addr_scalar) - trader_id) * is_active == 0` — identity is the on-chain settlement address, binding the proof to the account the contract debits/credits (#153). |

The two-sided range trick avoids an explicit absolute-value gadget:
because the Poseidon-friendly `signed_to_scalar` representation puts
positive integers in `[0, 2^60)` and negatives in `Fr - [1, 2^60]`, both
range checks together pin `new_pos` to the signed integer interval
`[-position_limit, position_limit]`.

## IVC state vector (`AuctionStepCircuit`)

The HyperNova step circuit carries a 5-element public state across rounds:

| # | Slot             | Meaning                                                        |
|---|------------------|----------------------------------------------------------------|
| 0 | `state_hash`     | Running `poseidon(prev, commitments_root, notionals_root, active_count)`. |
| 1 | `round_nonce`    | Increments by 1 each folded round.                             |
| 2 | `policy_hash`    | `poseidon(min_size, min_price, position_limit)`; invariant.    |
| 3 | `settlement_acc` | Hash-chain over each active match's `(bid_addr, ask_addr, price, size)`. `DarkPool.settleAuction` recomputes the identical Poseidon chain over `matches[]` and requires it equals `z_n[3]`, binding settlement to the proof (#209; `dp_zk::settlement_chain` + `PoseidonBN254.sol`). |
| 4 | `admit_chain`    | Hash-chain over each round's admitted-set Merkle root — binds the input set to the proof (#157). |

`z_0 = [0, 0, policy_hash, 0, 0]`. `verify_final` re-checks the whole
authenticated `z_0`/`z_n` slice, so neither chain can be rewritten while
presenting a valid proof.

## Input-completeness binding (#157)

The matching proof binds *match validity* and *which orders were settled*,
but on its own says nothing about *which orders were eligible*. A semi-trusted
operator could therefore match an order it never published (an off-log
phantom) against a victim. To close that, each round commits to the
**admitted set** — the commitments of every order live in the book when the
auction ran — as a canonical fixed-depth (`2^MERKLE_DEPTH`) Poseidon Merkle
root (`merkle::admitted_set_root`: leaves sorted by field value, padded with
the empty leaf). Family 5b proves every settled leg is a member of that root,
and the root is folded into `admit_chain` (`z[4]`).

A watcher reconstructs the same root from the public `OrderPlaced` /
`OrderCancelled` / expiry log (the per-round root is also published in the
`BatchFolded` event) and folds the chain; if it matches the proof's `z_n[4]`,
no off-log order was matched and the operator did not misrepresent the
admitted set.

**Scope boundary.** This binds *matched ⊆ admitted set* (no injection) and
makes the input set transparent. It does **not** prove the full fairness of the
off-circuit auction:

- **No volume-maximisation proof.** The circuit checks `bid_limit >=
  clearing_price >= ask_limit` for each supplied fill and checks that every
  active row uses the same `clearing_price`. It does not scan the admitted
  order book to prove this price maximizes matched volume.
- **No completeness / no-censorship proof.** Extra admitted orders may remain
  unmatched. Full maximal-matching is prohibitively expensive in this circuit,
  so censorship-by-omission remains detectable only by an external observer
  comparing the public book against the clearing price.
- **No price-time priority proof.** The matcher uses `seq` off-circuit, but
  `seq` is not a witness or public input here, so the proof cannot show that
  higher-priority eligible orders were filled before lower-priority orders.

Those properties are enforced by `dp-auction` and are replayable from the
event log, but they are not cryptographic constraints of the current proof.

## Poseidon vs. Pedersen

Despite the module name `pedersen`, the implementation is Poseidon over
BN254 `Fr` (rate 2, capacity 1, x⁵ S-box, 8 full + 57 partial rounds).
Reasons:

- Hiding + binding under the random-oracle assumption — same security
  envelope as Pedersen for our purposes.
- ~20× cheaper in-circuit than a literal Pedersen commitment over
  Jubjub (no curve arithmetic).
- Round constants and MDS matrix are deterministically derived via
  `find_poseidon_ark_and_mds`, so prover + verifier instantiate
  byte-identical configs.

## Version-bump policy

`CIRCUIT_VERSION` (in `src/lib.rs`) **must** be incremented whenever
`BatchProofCircuit::generate_constraints` changes — including new
witnesses, removed gates, reordered public inputs, or Poseidon-config
changes. Bumping invalidates all on-disk proving/verifying keys via
`KeyMetadata::check_compatible`. After a bump, regenerate dev keys:

```sh
cargo run --release -p dp-zk-cli --bin dp-zk-keygen -- \
    --batch-size 8 --out crates/dp-zk/keys --seed 1
```

Production deployments must instead run a multi-party Powers-of-Tau +
Phase 2 ceremony before publishing the new key set.
