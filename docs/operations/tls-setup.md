# TLS / mTLS for `darkpool-server`

`darkpool-server` terminates TLS in-process for both transports — the
tonic gRPC listener (default `:9090`) and the axum REST listener
(default `:8080`). A single cert/key pair covers both ports; mTLS is
opt-in via a separate client-CA bundle.

> 🔁 **Looking for ECIES decryption key rotation instead?** See
> [`docs/operations/key-rotation.md`](./key-rotation.md). That runbook
> rotates the key the operator uses to decrypt trader order payloads.
> This document is strictly about the transport-layer certificate
> presented to clients.

---

## Configuration surface

| Flag / env var | Meaning |
| --- | --- |
| `--tls-cert` / `DARKPOOL_TLS_CERT` | PEM cert (or chain). When set together with `--tls-key`, TLS is enabled on both listeners. |
| `--tls-key`  / `DARKPOOL_TLS_KEY`  | PEM private key matching `--tls-cert`. |
| `--tls-client-ca` / `DARKPOOL_TLS_CLIENT_CA` | PEM CA bundle that signs *client* certificates. Presence enables mTLS — clients without a cert chained to this bundle are rejected at the handshake. |

Validation rules (enforced at boot — invalid combinations exit
non-zero):

- Both `--tls-cert` and `--tls-key` empty → **plaintext mode**, with a
  loud `warn!` so it is impossible to ship to prod silently.
- Exactly one of `--tls-cert` / `--tls-key` set → fatal error.
- `--tls-client-ca` set without `--tls-cert`/`--tls-key` → fatal
  error (otherwise mTLS would silently degrade to "no TLS at all").

---

## Generating a dev certificate

Local development uses an ephemeral self-signed cert. The integration
tests mint these on the fly via `rcgen`; for a manual smoke test:

```sh
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout /tmp/dp.key.pem -out /tmp/dp.cert.pem \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,DNS:grpc.localhost,DNS:api.localhost,IP:127.0.0.1" \
  -days 7
chmod 0400 /tmp/dp.key.pem
```

Then boot:

```sh
DARKPOOL_TLS_CERT=/tmp/dp.cert.pem \
DARKPOOL_TLS_KEY=/tmp/dp.key.pem \
  cargo run -p dp-api
```

Smoke-test REST:

```sh
curl --cacert /tmp/dp.cert.pem https://127.0.0.1:8080/healthz
```

Smoke-test gRPC with `grpcurl`:

```sh
grpcurl -cacert /tmp/dp.cert.pem 127.0.0.1:9090 list
```

---

## Production deploy

1. Issue (or renew) a certificate covering every DNS name clients
   will dial. A single multi-SAN cert keeps the config surface small:

   ```text
   CN  = api.dp.example
   SAN = DNS:api.dp.example, DNS:grpc.dp.example
   ```

2. Install the material on the operator host with restrictive
   permissions:

   ```sh
   install -m 0400 -o dp-api -g dp-api new.cert.pem /etc/darkpool/tls/cert.pem
   install -m 0400 -o dp-api -g dp-api new.key.pem  /etc/darkpool/tls/key.pem
   ```

3. Point the service at the paths via `DARKPOOL_TLS_CERT` /
   `DARKPOOL_TLS_KEY` in `/etc/darkpool/env`.

4. Restart the service once. Subsequent renewals follow the
   **rotation** procedure below — no restart is required for REST.

---

## Enabling mTLS

Mutual TLS adds **client** cert verification to both listeners. The
operator runs the client CA; clients enroll by submitting a CSR to
ops, which signs it with the CA's private key.

1. Generate the client root CA (one-time):

   ```sh
   openssl req -x509 -newkey rsa:4096 -nodes -days 3650 \
     -subj "/CN=DarkPool Operator Client CA" \
     -keyout /etc/darkpool/tls/client-ca.key.pem \
     -out    /etc/darkpool/tls/client-ca.pem
   chmod 0400 /etc/darkpool/tls/client-ca.key.pem
   ```

2. Sign each trader's CSR with the client CA. Distribute the signed
   cert + the bundle of intermediates back to the trader.

3. Set `DARKPOOL_TLS_CLIENT_CA=/etc/darkpool/tls/client-ca.pem` and
   restart the service.

4. Validate end-to-end with `curl`:

   ```sh
   # mTLS-on server rejects anonymous clients.
   curl --cacert /etc/darkpool/tls/cert.pem \
     https://api.dp.example/healthz                          # fails
   # mTLS-on server accepts an enrolled client.
   curl --cacert /etc/darkpool/tls/cert.pem \
     --cert /etc/dp-trader/client.pem \
     --key  /etc/dp-trader/client.key.pem \
     https://api.dp.example/healthz                          # ok
   ```

---

## Rotation procedure

Cert rotation is asymmetric between the two listeners — REST hot-swaps,
gRPC requires a restart:

| Listener | Reload mechanism | Open connections |
| --- | --- | --- |
| REST (`:8080`)  | `SIGHUP` → in-place `RustlsConfig::reload_from_pem_file` (or `reload_from_config` for mTLS). | Kept alive on the **old** cert until they close; new handshakes use the **new** cert. |
| gRPC (`:9090`)  | None — tonic 0.12 exposes no reload API. **Restart the process** to swap the gRPC cert. | Dropped on restart. Clients reconnect transparently. |

### REST hot-reload (most rotations)

1. Atomically write the new material over the existing paths so the
   listener never observes a half-written file:

   ```sh
   install -m 0400 -o dp-api -g dp-api new.cert.pem /etc/darkpool/tls/cert.pem
   install -m 0400 -o dp-api -g dp-api new.key.pem  /etc/darkpool/tls/key.pem
   ```

2. Tell the running process to re-read them:

   ```sh
   systemctl reload dp-api      # systemd unit must send SIGHUP
   # or:
   kill -HUP "$(pidof darkpool-server)"
   ```

   Logs should show:

   ```text
   INFO SIGHUP: REST TLS material reloaded
   WARN SIGHUP: gRPC listener does not hot-reload TLS — restart the process to rotate the gRPC certificate
   ```

3. Confirm the new cert is being served:

   ```sh
   openssl s_client -connect api.dp.example:8080 -servername api.dp.example </dev/null \
     | openssl x509 -noout -subject -dates
   ```

### gRPC rotation (≤ quarterly)

Because the gRPC listener cannot hot-reload, schedule a graceful
restart to pick up the new cert:

```sh
systemctl restart dp-api
```

Cap the overall rotation cadence at **≤ 90 days** so Let's Encrypt is a
valid issuer if you ever stop running a private CA. Restart-windows
are short (sub-second handover when the upstream LB is configured to
retry on `UNAVAILABLE`).

### What if the SIGHUP reload fails?

The reload path is failure-soft: a bad cert file is logged and the
listener **keeps serving the previous certificate**. The relevant log
line is `WARN SIGHUP: REST TLS reload failed (keeping previous cert):
<reason>`. Diagnose the bad file, fix it, and re-send `SIGHUP`. No
service interruption occurs.

---

## Asymmetry — at a glance

```text
┌──────────┬────────────────────────┬─────────────────────┐
│ listener │ rotation cadence       │ reload mechanism    │
├──────────┼────────────────────────┼─────────────────────┤
│ REST     │ as often as needed     │ SIGHUP, in-place    │
│ gRPC     │ ≤ quarterly            │ process restart     │
└──────────┴────────────────────────┴─────────────────────┘
```

The asymmetry is documented honestly rather than papered over — the
gRPC side waits on a tonic upstream reload API. Tracking issue is
linked from the PR introducing this listener.

---

## Why these choices

- **App-native TLS (no sidecar).** Order payloads are decrypted in
  process; terminating TLS in the same binary keeps the
  confidentiality boundary co-located with the auth boundary. Ops are
  still free to front the service with nginx if they want, but the
  protocol guarantees do not depend on it.
- **Unified cert.** One cert/key pair covers both ports via SANs.
  Cuts the cert-issuance surface in half and keeps the env-var shape
  small (`--tls-cert`, `--tls-key`, `--tls-client-ca`). Per-listener
  override is a trivial follow-up if ops ever needs split DNS with
  split CAs.
- **rustls / ring.** Default crypto provider is `ring`; the same
  provider backs both the gRPC and REST listeners.
