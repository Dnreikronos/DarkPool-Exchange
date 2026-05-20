# Operator key rotation runbook

A rotation cycles the operator's ECIES decryption key without dropping
orders that are in-flight at the moment the new key takes effect.

The flow has three actors:

- **chain** — `DarkPool.operatorPubkey` is the discovery surface. The
  contract emits `OperatorPubkeyUpdated(oldPubkey, newPubkey, effectiveAt)`
  whenever the owner calls `setOperatorPubkey(bytes,uint64)`.
- **operator server** — `MultiKeyDecrypter` accepts any registered key.
  Status `Active|Rotating|Sunset` controls the try-order; the lowest
  status wins on a successful decrypt.
- **clients** — read the on-chain pubkey, encrypt to it. They neither
  see nor care that the operator runs a multi-key set during the drain
  window.

The procedure is deliberately split into five `curl`-grained steps so
each transition is auditable in shell history.

---

## 1. Generate the new ECIES keypair

```sh
dp-zk-rotate-key generate --out file:/etc/darkpool/keys/new.hex
```

Writes the 32-byte secret to the target file with mode `0600` and
prints the SEC1-compressed pubkey to stdout. Capture the pubkey hex —
you will hand it to the contract in step 2.

Use any `KeySource` URI scheme for storage of record. `file:` is the
only scheme `generate` writes to directly; for `age:` or `awskms:`
storage, wrap the resulting file with the appropriate tooling before
distributing it.

## 2. Publish the new pubkey on-chain

```sh
forge script script/Deploy.s.sol --rpc-url $RPC --private-key $OWNER_KEY \
  --sig 'run()' ...
```

…or directly via `cast`:

```sh
cast send $DARKPOOL "setOperatorPubkey(bytes,uint64)" \
  0x$NEW_PUBKEY_HEX $(date -d '+10 minutes' +%s) \
  --rpc-url $RPC --private-key $OWNER_KEY
```

`effectiveAt` is metadata — clients may use it to delay encrypting to
the new key until the operator has had time to register it server-side
(step 3). Pick a value 5–10 minutes ahead of the wall clock.

## 3. Hot-register the new key on the running operator

`dp-zk-rotate-key curl-spec` prints the exact command:

```sh
dp-zk-rotate-key curl-spec --id new-2026q2 \
  --uri file:/etc/darkpool/keys/new.hex --status rotating \
  --server https://operator.example.com
```

```text
curl -sS -X POST -H 'x-api-key: $OPERATOR_API_KEY' \
  -H 'content-type: application/json' \
  -d '{"id":"new-2026q2","uri":"file:/etc/darkpool/keys/new.hex","status":"rotating"}' \
  https://operator.example.com/v1/admin/keys
```

The new key joins the `MultiKeyDecrypter` set with status `Rotating`.
The previous key remains `Active`, so any client that has not yet
re-read chain state continues to encrypt to it without disruption.

When the operator is ready to promote the new key, re-POST the same
id with `"status":"active"`. The previous active key is **not**
automatically demoted — see step 4.

## 4. Demote the old key to `Sunset`

```sh
curl -sS -X POST -H "x-api-key: $OPERATOR_API_KEY" \
  https://operator.example.com/v1/admin/keys/$OLD_KEY_ID/sunset
```

`Sunset` keys continue to decrypt incoming ciphertexts — clients that
encrypted under the old key during the brief window before the on-chain
update propagated still get their orders accepted. New ciphertexts
will be encrypted to the `Active` key.

## 5. Drain to zero, then delete

Watch the per-key counter on Prometheus:

```promql
sum by (key_id) (rate(darkpool_crypto_decrypt_total{outcome="success"}[5m]))
```

Once the `key_id = $OLD_KEY_ID` series stops incrementing for at least
the longest TTL of an in-flight order (typically 1 minute, but configure
to your auction TTL plus a safety margin), delete the entry:

```sh
curl -sS -X DELETE -H "x-api-key: $OPERATOR_API_KEY" \
  https://operator.example.com/v1/admin/keys/$OLD_KEY_ID
```

Subsequent orders that arrive encrypted under the deleted key are
rejected with the freshest-key decrypt error (which is the new
`Active` key's, not the deleted one). This is the intentional contract:
operators see the same diagnostic users see, instead of a long-tail
"old key failed" message that has no actionable next step.

---

## Sizing the key set

Keep the active set to **three keys or fewer**:

- 1 × `Active`
- 1 × `Rotating` (during the propagation window)
- 1 × `Sunset` (during the drain window)

ECIES decryption is one AEAD-tag check (`< 1 ms` on commodity
hardware), but worst-case cost is O(N) per failed ciphertext. Two
overlapping drains would degrade tail latency on misencrypted orders.

## Things that look like rotation but aren't

- **Key compromise.** A leaked key has no drain window. Skip steps 2
  → Active and instead: revoke the old key first (`DELETE`), then
  publish a fresh key on chain and register it. Accept that any
  in-flight orders encrypted to the compromised key are dropped —
  that is the correct security trade-off.
- **Operator host migration.** No rotation needed. Copy the same key
  to the new host and start the server with the same URI.
- **VK/verifier rotation.** Unrelated; see `VerifierProxy.setVerifier`
  + ADR 0002. The operator decryption key does not gate proof
  verification.
