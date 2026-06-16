# Commitment-circuit proving key (DEMO — INSECURE)

`commitment_pk.bin` is the Groth16 proving key for the per-order
`CommitmentPreimageCircuit`. The in-browser prover
(`front/lib/prover`) fetches it and feeds it to `prove_order_wasm`, which
proves against this **canonical** key and returns only `(proof,
commitment)` — it never mints or ships a verifying key. See issue #212 for
why a prover-chosen VK is unsound.

## This key is deliberately insecure

It was generated from a **fixed, public seed**, so anyone can recreate the
trusted-setup trapdoor and forge proofs under the matching verifying key:

```
cargo run -p dp-zk-cli --bin dp-zk-cli -- \
  setup-commitment-circuit \
  --out <dir> --seed 212 --allow-insecure-seed-for-fixtures
```

It exists only so the proving UX works end-to-end before a real ceremony.
Do **not** configure the engine with the matching demo `commitment_vk.bin` in
any real environment: because the seed is public, anyone can forge proofs that
verify under that VK. Use `DARKPOOL_ALLOW_UNVERIFIED_ORDER_PROOFS=true` only
for local/dev fixtures, or replace this key with ceremony output.

## Replacement

A real, single-contributor-honest trusted setup produces the canonical
`(commitment_pk.bin, commitment_vk.bin)` pair. The proving key replaces this
file, and `darkpool-server` must be started with `DARKPOOL_ORDER_PROOF_VK`
pointing at the matching verifying key.
