# Dependency Versions

## Sonobe (HyperNova IVC)

- **Crate**: `folding-schemes`
- **Source**: https://github.com/privacy-scaling-explorations/sonobe
- **Pinned SHA**: `63f2930d363150d4490ce2c4be8e0c25c2e1d92c`
- **Why pinned**: pre-audit library; pin to a known-good commit for reproducibility

## Arkworks Patches

Sonobe requires patched versions of two arkworks crates that add GR1CS support:

| Crate | Source | Reason |
|---|---|---|
| `ark-relations` | `github.com/arkworks-rs/snark` (main) | Adds `gr1cs` module for HyperNova |
| `ark-r1cs-std` | `github.com/flyingnobita/r1cs-std_yelhousni@b4bab0c` | Perf fork with GR1CS support |

## Decider Trusted Setup (SRS)

- The HyperNova Decider circuit requires a one-time KZG SRS.
- The Ethereum EIP-4844 KZG ceremony SRS is valid for this purpose.
- Source: https://ceremony.ethereum.org
- This SRS is used **only for the Decider circuit**, not for the `AuctionStepCircuit` itself.
  HyperNova's folding steps require no trusted setup.
