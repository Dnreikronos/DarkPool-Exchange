# Committed ABI snapshots

These JSON files are bare `abi` arrays extracted from the Foundry build
artifacts in `contracts/out/`. `src/abi.rs` binds against them via
`alloy_sol_types::sol!` so the Rust build does not depend on Foundry.

A drift test (`tests/abi_drift.rs`) compares each snapshot to the live
artifact when `contracts/out/` is present, and skips otherwise.

## Regenerate

After changing a Solidity contract:

```sh
cd contracts && forge build && cd ..
jq '.abi' contracts/out/DarkPool.sol/DarkPool.json           > crates/dp-settlement/abi/DarkPool.json
jq '.abi' contracts/out/VerifierProxy.sol/VerifierProxy.json > crates/dp-settlement/abi/VerifierProxy.json
```

Snapshot only the `abi` field, not the full Foundry object, so bytecode
churn from `via_ir`/optimizer toggles does not pollute commits.
