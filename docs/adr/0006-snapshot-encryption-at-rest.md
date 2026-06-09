# ADR 0006 — Snapshot encryption at rest

- **Status:** Accepted
- **Date:** 2026-06-08
- **Issue:** [#203](https://github.com/Dnreikronos/DarkPool-Exchange/issues/203)

## Context

The engine periodically writes state snapshots to the `SnapshotStore` so
recovery can skip a full event replay. Snapshots were serialized as **bare
bincode**: the blob carried the live order book — every `Order` with cleartext
`trader`, `price`, `size`, `commitment_key` — plus `pending_batches[]
.settlement_matches[]` with cleartext `bid_trader` / `ask_trader` / `price` /
`size`. The envelope's SHA-256 was integrity only. Anyone with read access to
the snapshot store read resting-order and settled-match data directly — a
larger privacy-at-rest leak than the salt_nonce concern in #177, and one the
`event_store_contains_no_plaintext` canary never covered (it walks only the
event log).

## Decision

Seal the entire serialized snapshot with an AEAD before it touches the store.

- **Cipher:** XChaCha20-Poly1305. The 192-bit random nonce per snapshot rules
  out nonce reuse without counter state, which suits independently-encrypted
  blobs. The Poly1305 tag replaces the SHA-256 (authenticity *and* integrity).
- **Dedicated key, not derived from the operator ECIES key.** Resolved via the
  existing `KeySource` URI machinery (`file:` / `age:` / `awskms:`) from
  `--snapshot-key-uri` / `DARKPOOL_SNAPSHOT_KEY_URI`.

### Why a dedicated key over deriving one from the operator key

Both options protect against the core threat (store-read access alone reveals
nothing). The dedicated key wins on trade-offs:

1. **Key separation.** A leaked snapshot key cannot decrypt live order
   ciphertext, and a leaked operator ECIES key cannot read snapshots. Distinct
   blast radii.
2. **Rotation safety — decisive.** The protocol supports operator pubkey
   rotation (`setOperatorPubkey`, `MultiKeyDecrypter`). A snapshot key derived
   from the active ECIES key would make pre-rotation snapshots undecryptable;
   combined with event compaction behind them, recovery would hit
   `SnapshotsCorruptAndLogTruncated` and **refuse to boot**. A dedicated key
   rotates on its own schedule and never entangles the two lifecycles.
3. **Minimal surface.** Reuses `KeySource` verbatim — one boot-time knob, no new
   key infrastructure, and no need to re-expose the operator secret that
   `dp-crypto` deliberately keeps zeroized and private.

The only cost — one extra secret to provision — is outweighed by avoiding a
boot-availability footgun on a first-class protocol flow.

### Envelope format (magic `DPS1` → `DPS2`)

```
magic "DPS2" (4) | version=2 (4 BE) | seq (8 BE) | sealed_len (4 BE) | sealed
sealed = nonce(24) || ciphertext+tag
```

`magic || version || seq` is the AEAD associated data, so neither the version
nor the seq can be altered without failing the tag. A stray `DPS1` (plaintext)
envelope fails `BadMagic` → treated as corrupt → recovery falls back to event
replay. Greenfield: no back-compat shim.

### Fail-closed boot policy

- **Durable store (file / postgres) without a key URI →** the server refuses to
  start. It will not write plaintext order data at rest.
- **Durable store with a key URI →** load the key, seal every snapshot.
- **In-memory store (non-durable) without a key URI →** generate a per-process
  ephemeral key. Those snapshots never survive a restart anyway, so an ephemeral
  key has no recovery downside while still keeping at-rest bytes encrypted.

The engine mirrors this: the snapshotter refuses to run, and `take_snapshot`
returns `CipherMissing`, when a store is wired without a cipher.

## Consequences

- Operators running a durable snapshot store must provision and protect a
  second secret (`DARKPOOL_SNAPSHOT_KEY_URI`). Losing it makes existing
  snapshots unrecoverable — recovery then needs an intact event log from seq 1.
- Rotating the snapshot key makes envelopes sealed under the old key
  undecodable; treat a rotation like a compaction boundary (keep an event tail,
  or accept a full replay on the next boot).
- The `snapshot_contains_no_plaintext` canary now reads raw envelope bytes and
  asserts secret markers are absent, with a sanity check that the *unencrypted*
  form would contain them — so this path can no longer regress silently.
