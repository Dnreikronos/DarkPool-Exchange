# Trader ID projection

`useTraderId()` projects the connected wallet address into the
**commitment-key string** that the ZK prover absorbs when deriving the
canonical trader id.

## Projection

```
address                                 → useTraderId()
0x1111111111111111111111111111111111111111 → "1111111111111111111111111111111111111111"
```

The transform is two steps:

1. Strip the `0x` prefix.
2. Lowercase the remaining hex.

The result is the **commitment key string** — not the trader id itself.
The trader id is computed inside the prover (and matched by the
circuit) as:

```
trader_id = poseidon(Fr::from_be_bytes_mod_order(utf8_bytes(commitment_key)))
```

See `crates/dp-zk/src/pedersen.rs::derive_trader_id` for the canonical
implementation and `crates/dp-zk/src/witness.rs::OrderLegWitness` for
the wire shape (`trader_id` is the hex-encoded 32-byte BE Poseidon
output; `commitment_key` is the string the frontend supplies).

## Why this exact projection

The prover absorbs `utf8_bytes(commitment_key)` directly via
`Fr::from_be_bytes_mod_order`. The byte sequence on the wire must match
byte-for-byte across:

- This frontend hook (`useTraderId()`)
- Any prover input we build later (`front/lib/prover/`, issues #97/#98)
- The Rust circuit's `derive_trader_id` (already merged)

Lowercase + no `0x` is the convention chosen because:

- EIP-55 checksummed addresses introduce case variance per address;
  collapsing to lowercase removes that variance.
- The `0x` prefix is a presentational hex marker, not part of the
  20-byte address value. Stripping it keeps the commitment-key bytes
  aligned with the on-chain address bytes.
- The resulting string is exactly 40 ASCII hex characters — a
  predictable byte length for downstream encoders.

## Stability contract

This projection is part of the wallet module's public API. If the
projection changes, the prover input builder, the circuit witness, and
any persisted indexer state all need to migrate in lockstep. Do not
change `normalizeTraderId` without raising an ADR.
