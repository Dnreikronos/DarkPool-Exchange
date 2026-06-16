# Trader identity projection

`useTraderId()` projects the connected wallet address into the lowercase,
prefix-free hex string used by wallet-scoped UI state.

## Projection

```
address                                      -> useTraderId()
0x1111111111111111111111111111111111111111  -> "1111111111111111111111111111111111111111"
```

The transform is two steps:

1. Strip the `0x` prefix.
2. Lowercase the remaining hex.

This string is still useful for local indexes and display-adjacent code, but it
is no longer the ZK prover's identity preimage.

## Prover binding

The order proof binds identity to the connected on-chain address bytes:

```
trader_id = poseidon(Fr::from_be_bytes_mod_order(address_bytes))
```

The frontend witness builder sends the 0x-prefixed wallet address as
`trader_addr`. The WASM prover strips the prefix, decodes the 20 address bytes,
and derives the same `trader_id` that the engine derives from
`DecryptedOrder.trader`.

`commitment_key` remains in the encrypted order payload as client-chosen
entropy, but it does not identify the trader in the proof.

## Stability contract

The witness builder, WASM prover, engine derivation, and commitment circuit must
stay in lockstep. If address normalization changes, update all four together
and record the migration in an ADR.
