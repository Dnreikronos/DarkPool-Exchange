#!/usr/bin/env bash
#
# Prints a sha256 over the sources that determine the dp-zk-wasm browser
# artifacts committed under front/lib/prover/zk-pkg/.
#
# Why a source hash and not a diff of the artifacts themselves: the
# wasm-bindgen glue (.js/.d.ts) is reproducible across machines, but the
# compiled .wasm is not — Rust patch version, LLVM version and embedded
# paths all move the bytes. Comparing the binary directly fails on an
# unrelated toolchain bump while still missing nothing a source hash
# catches, so the hash is both stricter and quieter.
#
# `just build-wasm-zk` records the value in front/lib/prover/zk-pkg/BUILDINFO;
# the WASM (dp-zk-wasm) CI job recomputes it and fails when it drifts. That
# is what stops a circuit change from shipping against a stale prover.
set -euo pipefail

cd "$(dirname "$0")/.."

# Cargo.lock pins the external crates (arkworks et al); Cargo.toml carries
# the workspace dependency versions. The three crates are the path-dependency
# closure of dp-zk-wasm.
INPUTS=(
  Cargo.lock
  Cargo.toml
  crates/dp-poseidon
  crates/dp-zk
  crates/dp-zk-wasm
)

# The closure above is written by hand, so guard it: if any of those crates
# gains a path dependency that is not already an input, this hash would go
# blind to that crate's sources — the exact failure mode the check exists to
# prevent. Fail loudly instead of hashing an incomplete set.
declared_path_deps() {
  grep -ho 'path = "\.\./[a-z0-9_-]*"' \
    crates/dp-zk-wasm/Cargo.toml \
    crates/dp-zk/Cargo.toml \
    crates/dp-poseidon/Cargo.toml 2>/dev/null |
    sed 's|path = "\.\./||; s|"||' | sort -u
}

for dep in $(declared_path_deps); do
  found=0
  for input in "${INPUTS[@]}"; do
    [ "$input" = "crates/$dep" ] && found=1
  done
  if [ "$found" -eq 0 ]; then
    echo "zk-artifact-hash: crates/$dep is a path dependency of the prover" >&2
    echo "  but is not in INPUTS — add it to $0 and regenerate BUILDINFO." >&2
    exit 1
  fi
done

# Hash the working-tree contents of the tracked files, not the committed
# blobs: `just build-wasm-zk` must record what it actually compiled, which
# includes uncommitted edits. `git ls-files` output is sorted, and sha256sum
# includes each path in its output, so the result is order-stable.
git ls-files -z -- "${INPUTS[@]}" | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
