# ADR 0003 — Operator key rotation & secure key management

- **Status:** Accepted
- **Date:** 2026-05-19
- **Issue:** [#28](https://github.com/Dnreikronos/DarkPool-Exchange/issues/28)

## Context

Until this change the operator's ECIES decryption key was a raw 32-byte
hex file loaded into a single `EciesDecrypter` at boot. Three problems
flowed from that:

1. Rotating the key required restarting the operator and dropped any
   ciphertexts already in-flight at the cutover, because the new
   decrypter couldn't open them.
2. The Ethereum-signing key (`EthSubmitterConfig.private_key: String`)
   and the ECIES decryption key were stored the same way and routed
   through the same env var idiom, which conflated two unrelated
   secrets and made a "swap them in production for a KMS-backed
   variant" follow-up impossible without breaking the trait surface.
3. Clients had no way to discover the current operator pubkey: it was
   announced out-of-band or hard-coded into the SDK at release time.

Issue #28 wants: rotation with a drain window, secure storage
backends, separated trait surfaces for the two secrets, and on-chain
discovery.

## Decision

### 1. `MultiKeyDecrypter` is the unit of decryption

Instead of `Engine::set_decrypter(EciesDecrypter)`, the engine receives
a `MultiKeyDecrypter` — a `Vec<KeyEntry>` behind an `Arc<RwLock>` where
each entry carries `id`, `status ∈ {Active, Rotating, Sunset}`, and an
inner `Arc<dyn Decrypter>`. Iteration order is `Active → Rotating →
Sunset`. The first `Ok` wins; if every entry fails, the freshest
(highest-priority) error propagates.

The freshest-error rule matters for diagnosis: when a client tries to
encrypt under a deleted key, the operator should see "the active key
rejected this", not "the long-since-sunset key rejected this". The
former is actionable (regenerate ciphertext); the latter is dead-end
debugging.

The `KeyStatus` lifecycle is exposed on the admin API but no further:
operators choose when to promote (`Rotating → Active`), demote
(`Active → Sunset`), or delete. The contract has no opinion about
which key is "current" on the server — only about which pubkey
clients should encrypt to.

### 2. The signer and the decrypter are independent

`TxSigner` is a separate trait from `Decrypter`. `EthSubmitterConfig`
no longer carries `private_key: String`; callers pass an
`Arc<dyn TxSigner>` that exposes only `address()` and `wallet()`. KMS
signers (when wired) implement `TxSigner` directly so the raw private
key never leaves the KMS process boundary.

The trait surface is intentionally tiny — the submitter never needs
"sign this arbitrary blob" semantics; alloy's `EthereumWallet` is
already the abstraction that owns transaction signing in-process.

### 3. URI-driven storage backends

Both `KeySource` (ECIES) and `TxSigner::from_uri` (signer) parse
URI-style configs:

| Scheme       | Backend                                                                 |
|--------------|-------------------------------------------------------------------------|
| `file:`      | Plaintext hex on disk (current behaviour, kept for dev).                |
| `age:`       | age-encrypted file, passphrase from `DARKPOOL_KEY_PASSPHRASE` /         |
|              | `DARKPOOL_SIGNER_PASSPHRASE`.                                            |
| `awskms:`    | Feature-gated scaffold. KMS-decrypts a wrapped DEK for ECIES; uses an    |
|              | `AwsSigner` (ECDSA inside KMS) for the signer.                           |

The ECIES path **transports the secret through KMS**, rather than
asking KMS to perform ECIES. KMS only natively supports ECDSA over
secp256k1; ECDH over the same curve (which ECIES needs) is not a
first-class KMS operation, so the decryption secret has to live in
the operator process. The trade-off is acceptable: the KMS Decrypt
call replaces an at-rest secret on disk; the in-memory window for
the secret is bounded by process lifetime and `EciesDecrypter`'s
`Drop`-time zeroize.

For the signer, KMS performs the ECDSA itself — the private key
never reaches the operator. The two backends differ on purpose.

### 4. The pubkey lives on chain

`DarkPool.operatorPubkey` (SEC1-encoded `bytes`) is the canonical
discovery channel. The contract emits
`OperatorPubkeyUpdated(oldPubkey, newPubkey, effectiveAt)` on every
update. SDKs read the storage slot (cheap) or subscribe to the event
(decentralised); either way the operator never has to publish a fresh
release to rotate the discovery surface.

The `effectiveAt` field is metadata — the contract does not gate any
state transition on it. Clients use it to delay encrypting to the new
key by a few minutes, giving the operator time to register the new
key server-side via `POST /v1/admin/keys` before any client switches.

`DarkPool.sol`'s constructor now takes the initial pubkey and emits
the initial event so the on-chain history is complete from block 0.

## Alternatives considered

### Per-key engine instances (rejected)

We could have stood up one `Engine` per active key and load-balanced
ciphertexts across them. Rejected because the engine is also the
matching engine — orderbook state, auction tick, salt nonce. Spawning
two engines for a 5-minute rotation window doubles every other
resource the engine owns; it also forces a re-merge of two
independent auction streams at drain time, which is a substantial
correctness surface for a feature that only needs to decrypt under
multiple keys.

The single `MultiKeyDecrypter` keeps the rest of the engine
oblivious to rotation — `Engine::set_decrypter` still takes one
`Arc<dyn Decrypter>`.

### Off-chain pubkey announcement (rejected)

A signed JSON payload on a CDN would have worked for the discovery
problem. Rejected because clients already need an L1 connection to
deposit; pulling the pubkey from the same contract avoids a second
trust surface and a separate signing-key story for the announcement
file. The cost is one extra storage slot and one event topic, paid
once at deploy plus once per rotation — cheap.

### In-KMS ECIES (rejected as out-of-scope)

A custom KMS implementation could expose secp256k1 ECDH primitives.
This would let the ECIES secret never leave KMS at all. The current
hosted KMS surfaces (AWS, GCP) do not expose secp256k1 ECDH; building
our own KMS for this is not justified by the threat model for the
MVP.

If the threat model later requires never holding the ECIES secret in
the operator process, the path is either (a) a custom HSM with the
right primitives or (b) re-keying away from ECIES towards a scheme
KMS can implement (e.g. ECIES over a curve KMS supports natively).
Both are deferred.

### `--operator-key` is kept (single-key compatibility)

`DARKPOOL_OPERATOR_KEY` still works for development and tests where
operators don't want to plumb a URI for a single key. The boot path
wraps the single key in a `MultiKeyDecrypter` with status `Active` and
id `primary`, so the admin endpoints work in single-key mode too.

## Consequences

- The deploy script (`contracts/script/Deploy.s.sol`) now requires
  `OPERATOR_PUBKEY_HEX`. Mainnet deploys must set it; the constructor
  rejects empty pubkeys.
- The ABI on `dp-settlement` (`crates/dp-settlement/abi/DarkPool.json`)
  changes — `setOperatorPubkey`, `operatorPubkey`,
  `operatorPubkeyEffectiveAt`, `OperatorPubkeyUpdated`. Drift test
  catches future re-emits.
- `EthSubmitterConfig.private_key: String` is removed. Callers must
  build a `TxSigner` via `dp_settlement::signer::from_uri(uri)` and
  pass it in. The integration test at
  `crates/dp-settlement/tests/anvil_e2e.rs` shows the pattern.
- New metric `darkpool_crypto_decrypt_total{key_id,status,outcome}`
  is the signal operators use to confirm a sunset key has drained.
- `dp-zk-rotate-key` ships alongside the existing dp-zk-cli binaries
  to script the `generate → publish → register → sunset → delete`
  cycle.
- HSM (PKCS#11) is out of scope. KMS is the only remote backend.
  The `awskms:` URI scheme is wired but the body is a scaffold
  pending the next iteration.
