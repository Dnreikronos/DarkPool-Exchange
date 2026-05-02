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

## Constraint families (v2)

| # | Family                      | Active-only? | Mechanism                                           |
|---|-----------------------------|--------------|-----------------------------------------------------|
| 1 | Side bit-form               | All rows     | `side * (1 - side) == 0` per leg.                   |
| 2 | Opposite sides              | Active rows  | `(bid_side + ask_side - 1) * is_active == 0`.       |
| 3 | Crossing                    | Active rows  | `bid_lp - match_price` and `match_price - ask_lp` non-negative via 60-bit range. |
| 4 | Min-size, min-price         | All / active | 60-bit ranges on `size`, `price`; diff to floors gated by `is_active`. |
| 5 | Leg commitment binding      | All rows     | In-circuit Poseidon over `(trader_id, side, lp, size, salt)` reconstructs the commitment, accumulated into `commitments_root`. |
| 6 | Notional binding            | All rows     | `match_price * match_size` accumulates into `notionals_root`. |
| 7 | Solvency                    | Active rows  | `balance < 2^60` (60-bit range) AND `balance * 1e8 - notional` fits 120-bit range. |
| 8 | Position-limit (two-sided)  | Active rows  | For `new_pos = position ± match_size`, `(limit - new_pos)` and `(limit + new_pos)` each fit in 60 bits → `\|new_pos\| ≤ position_limit`. |
| 9 | Trader-id binding           | Active rows  | `(poseidon(commitment_key_scalar) - trader_id) * is_active == 0`. |

The two-sided range trick avoids an explicit absolute-value gadget:
because the Poseidon-friendly `signed_to_scalar` representation puts
positive integers in `[0, 2^60)` and negatives in `Fr - [1, 2^60]`, both
range checks together pin `new_pos` to the signed integer interval
`[-position_limit, position_limit]`.

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
