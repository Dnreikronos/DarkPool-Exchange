# dp-zk

Zero-knowledge batch-proof primitives for DarkPool's auction pipeline.
Proves the validity of a batch of matched-pair fills (crossing, uniform
settlement price, admitted-set membership, solvency, position-limit,
trader-id binding) without revealing trader identities, balances, salts, or
pre-trade positions.

This crate does not prove full matching fairness. Volume-maximising price
selection, completeness/no-censorship, and price-time priority are implemented
by the off-circuit matcher and are auditable from the event log; they are not
constraints in the current circuit. See [`CIRCUIT.md`](./CIRCUIT.md).

## Layout

- `src/circuit/` — Groth16 circuit over BN254 (`BatchProofCircuit`).
- `src/encoding.rs` — deterministic `Decimal ↔ Fr` mapping (1e8 scale,
  60-bit cap).
- `src/witness.rs` — serializable witness types shared with
  `dp-zk-cli`.
- `src/keys/` — ark-serialize wrappers + on-disk metadata sidecar.
- `src/pedersen.rs` — native + in-circuit Poseidon helpers (the name
  is kept for spec parity; implementation is Poseidon — see
  `CIRCUIT.md`).
- `keys/` — dev-only proving/verifying keys committed for local tests.
  **Do not use in production.**

## Dev keys vs. production keys

The keys checked into `crates/dp-zk/keys/` come from a deterministic,
single-machine setup driven by `dp-zk-keygen`. They exist solely to
make local tests (`zk_e2e`, `zk_pipeline`) self-contained. They are
**not** safe for production:

- Single-party setup means whoever runs keygen learns the trapdoor.
- Anyone holding the trapdoor can forge proofs that pass
  verification.
- Production deployments must run a multi-party
  [Powers-of-Tau](https://eprint.iacr.org/2017/1050) + Phase 2
  ceremony so no participant can recover the trapdoor.

`CIRCUIT_VERSION` (in `src/lib.rs`) is bumped whenever the constraint
system changes; the on-disk metadata's `circuit_version` must match
or `check_compatible()` rejects the keys.

## Local commands

Generate dev keys for batch size 8 (default for `dp-zk-cli`):

```sh
cargo run --release -p dp-zk-cli --bin dp-zk-keygen -- \
    --batch-size 8 --out crates/dp-zk/keys --seed 1
```

Run the unit tests (constraint satisfiability + per-family negative
cases):

```sh
cargo test -p dp-zk
```

Run the prove-time smoke (ignored by default; ~30s budget):

```sh
cargo test --release -p dp-zk -- --ignored
```

End-to-end prove + verify through the subprocess prover:

```sh
cargo build --release -p dp-zk-cli
cargo test -p dp-aggregator --test zk_e2e
cargo test -p dp-engine    --test zk_pipeline
```

## Production-ceremony pointer

The production key ceremony is out of scope for this crate. The plan
of record is the standard two-phase BN254 ceremony — a generic
Powers-of-Tau followed by a circuit-specific Phase 2 keyed off the
exact `BatchProofCircuit` shape at the deployed `CIRCUIT_VERSION`. The
output replaces `crates/dp-zk/keys/` for production builds; the dev
keys must never ship with a release artefact.
