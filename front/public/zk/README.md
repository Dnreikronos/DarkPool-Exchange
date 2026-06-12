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
It is safe **today** because nothing verifies the per-order proof at
ingestion — the engine re-derives order validity operator-side (see
`dp-engine::place_encrypted_order` and ADR 0001). Do not rely on it for any
security property.

## Replacement

A real, single-contributor-honest trusted setup is tracked in #97/#98.
That ceremony produces the canonical `(commitment_pk.bin,
commitment_vk.bin)` pair; the proving key replaces this file and the engine
pins the matching verifying key. The VK for this demo key is reproducible
from the same command above (`commitment_vk.bin`, ~330 B) and is not
checked in here because no verifier consumes it yet.
