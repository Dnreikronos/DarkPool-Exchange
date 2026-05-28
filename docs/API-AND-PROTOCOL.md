# DarkPool Exchange: API and protocol reference

Quick reference for anyone integrating with the exchange or just trying to understand how pieces fit together. Covers the REST/gRPC API, the ZK proof system, and the on-chain settlement contracts.

---

## How orders work, in 30 seconds

A trader encrypts their order to the operator's public key, attaches a ZK proof that the order is valid, and submits it. The operator can't see the order contents until decryption happens server-side. Every 5 seconds, the engine runs a batch auction: computes a clearing price, matches orders, generates a proof that matching was done correctly, and settles on-chain. Nobody in the mempool ever sees the orders.

---

## API endpoints

Trading endpoints accept either an `x-api-key` header or an `Authorization: Bearer <jwt>` token (see [Authentication](#authentication) below). Admin endpoints use a separate operator key. The API runs REST on port 8080 and gRPC on port 9090.

### Authentication

Two modes, selected at boot via `DARKPOOL_SIWE_ENABLED`:

**API keys (default).** Static keys set via `DARKPOOL_API_KEYS`. All requests carry `x-api-key: <key>`. No per-user identity on the backend.

**SIWE wallet auth (opt-in).** Traders authenticate by signing an EIP-4361 message with their wallet. The server issues a JWT bound to the wallet address. Both modes work simultaneously when SIWE is enabled.

#### `GET /v1/auth/nonce` - Get a nonce

Returns a single-use nonce for constructing a SIWE message. Nonces expire after 5 minutes. The server caps pending nonces at 10k and returns 429 if exhausted.

**Response:**
```json
{ "nonce": "kEWepMt9knR6lWJ6A" }
```

#### `POST /v1/auth/verify` - Verify signature, get JWT

Submit the signed EIP-4361 message. The server verifies the signature, validates domain/chain/expiration/not-before, consumes the nonce, and returns a JWT.

**Request:**
```json
{
  "message": "<EIP-4361 plaintext message>",
  "signature": "0x<65-byte hex>"
}
```

**Response:**
```json
{
  "token": "eyJ...",
  "expires_at": 1716829200,
  "address": "0x6da0..."
}
```

The JWT is HS256-signed with `iss`/`aud` set to `darkpool`. Default TTL is 24 hours (`DARKPOOL_SESSION_TTL`). Pass it on subsequent requests as `Authorization: Bearer <token>`.

#### Identity binding

When a wallet-authenticated trader places an order, the engine verifies the decrypted `trader` address matches the session address after ECIES decryption. Mismatches return 403. This binds the full identity chain: wallet signature, encrypted order, commitment key, and ZK circuit `trader_id`.

API key callers skip this check (no wallet identity to bind against).

#### Rate limiting

The rate limiter keys on wallet address for SIWE sessions, API key value for key auth, or IP as fallback.

#### Configuration

| Env var | Default | Purpose |
|---------|---------|---------|
| `DARKPOOL_SIWE_ENABLED` | `false` | Enable wallet auth |
| `DARKPOOL_SESSION_SECRET` | (required if enabled) | JWT signing secret, minimum 32 bytes |
| `DARKPOOL_SESSION_TTL` | `24h` | JWT lifetime |
| `DARKPOOL_SIWE_DOMAIN` | (empty) | Expected SIWE message domain. Rejects cross-site replays when set |

---

### Trading (public)

#### `POST /v1/orders` - Place an order

Submit an encrypted order with its ZK commitment proof.

**Request:**
```json
{
  "commitment": "<base64 bytes>",
  "proof": "<base64 bytes>",
  "encrypted_payload": "<base64 bytes>"
}
```

**Response:**
```json
{
  "order": {
    "id": "uuid",
    "pair": "ETH/USDC",
    "side": "SIDE_BUY",
    "price": "1850.50",
    "size": "2.5",
    "remaining_size": "2.5",
    "commitment_key": "...",
    "submitted_at_unix": 1716825600,
    "expires_at_unix": 1716829200
  }
}
```

The engine decrypts server-side using the operator's ECIES key. When the caller is wallet-authenticated, the engine also verifies the decrypted `trader` field matches the session address (see [Identity binding](#identity-binding)).

---

#### `DELETE /v1/orders/{order_id}` - Cancel an order

Optional query param `reason`. Returns empty body on success.

---

#### `GET /v1/orders/{order_id}` - Get order details

Returns the same `OrderInfo` shape as the place-order response. 404 if the order doesn't exist.

---

#### `GET /v1/orderbook?pair=ETH/USDC` - Order book

**Response:**
```json
{
  "pair": "ETH/USDC",
  "bids": [
    { "price": "1850.00", "total_size": "12.5", "order_count": 4 }
  ],
  "asks": [
    { "price": "1851.00", "total_size": "8.3", "order_count": 2 }
  ]
}
```

Aggregated by price level. Sizes and prices are always strings (decimal, not floats).

---

#### `GET /v1/auctions?pair=ETH/USDC&limit=50` - Auction history

**Response:**
```json
{
  "auctions": [
    {
      "auction_id": "uuid",
      "pair": "ETH/USDC",
      "clearing_price": "1850.75",
      "matched_volume": "45.2",
      "match_count": 12,
      "timestamp_unix": 1716825600
    }
  ]
}
```

Returns past auction results. `limit` defaults to 50 if omitted.

---

#### `GET /v1/auctions/stream?pair=ETH/USDC` - Live auction stream

Server-Sent Events over REST, or gRPC server streaming. Each event is an `AuctionSummary` (same shape as above). If the stream falls behind, you'll get a `{"lagged": N}` event telling you how many you missed. Reconnect when that happens.

---

#### `GET /v1/pairs` - List trading pairs

**Response:**
```json
{
  "pairs": [
    {
      "pair": "ETH/USDC",
      "base_token": "0x...",
      "quote_token": "0x...",
      "min_order_size": "0.01",
      "tick_size": "0.01",
      "auction_interval_ms": 5000,
      "status": "PAIR_STATUS_ACTIVE"
    }
  ]
}
```

Only shows active pairs. Suspended/delisted ones are hidden from this endpoint.

---

#### `GET /v1/operator/pubkey` - Operator encryption key

**Response:**
```json
{
  "pubkey": "04abcdef...",
  "encoding": "sec1-uncompressed"
}
```

This is the ECIES public key traders encrypt their orders to. Cached for 5 minutes. Returns 503 if no active key is configured.

---

### Admin (operator only)

These live under `/v1/admin/*` and are REST-only (deliberately not mounted on the gRPC port so traders can't reach them).

| Method | Path | What it does |
|--------|------|-------------|
| `POST` | `/v1/admin/pairs` | Register a new trading pair. Body: `pair`, `base_token`, `quote_token`, optional `min_order_size`, `tick_size`, `auction_interval_ms` |
| `PATCH` | `/v1/admin/pairs/{pair}/suspend` | Pause trading on a pair without cancelling open orders |
| `DELETE` | `/v1/admin/pairs/{pair}` | Delist a pair permanently, cancels all open orders. Response includes `cancelled_orders` count |
| `GET` | `/v1/admin/pairs` | List all pairs including suspended/delisted ones |
| `POST` | `/v1/admin/keys` | Register or update an operator decryption key. Body: `id`, `uri` (file:, awskms:, etc.), optional `status` |
| `GET` | `/v1/admin/keys` | List all registered keys with their status |
| `DELETE` | `/v1/admin/keys/{key_id}` | Remove a key. Returns 204 |
| `POST` | `/v1/admin/keys/{key_id}/sunset` | Mark a key as sunset (stops accepting new orders encrypted to it) |

---

### Ops (unauthenticated)

| Path | Purpose |
|------|---------|
| `GET /healthz` | Always returns `{"status": "ok"}`. For load balancers |
| `GET /readyz` | Checks store and aggregator. Returns 503 with `{"status": "not_ready", "failed": "<probe>"}` if something's down |
| `GET /metrics` | Prometheus scrape endpoint |
| `GET /v1/auth/nonce` | SIWE nonce (only mounted when `DARKPOOL_SIWE_ENABLED=true`) |
| `POST /v1/auth/verify` | SIWE signature verification + JWT issuance (same gate) |

---

## The ZK proof system

Two layers of proofs, each doing something different.

### Layer 1: order commitments (Groth16)

When a trader submits an order, they also submit a Groth16 proof over BN254 that they know the preimage of a Poseidon hash commitment:

```
poseidon(trader_id, side, limit_price, order_size, salt) == commitment
```

The commitment is the only public input. Everything else stays private. This binds the encrypted order contents to a public value without revealing what's inside.

### Layer 2: batch matching (HyperNova IVC)

The operator runs batch auctions. For each batch, they fold a step through a HyperNova IVC circuit that checks up to 256 matches at a time. The circuit enforces 9 constraint families per match:

1. **Side validity** - bid/ask flags are binary
2. **Side opposition** - each active match has one bid and one ask
3. **Price crossing** - bid price >= match price >= ask price
4. **Size and price bounds** - 60-bit range checks on match size and prices
5. **Commitment binding** - Poseidon hashes of each order leg match what was submitted
6. **Notional calculation** - notional = price * size
7. **Solvency** - trader balance covers the notional (120-bit range check)
8. **Position limits** - new positions stay within symmetric bounds
9. **Trader identity** - trader_id = poseidon(commitment_key)

The IVC state carries three field elements across steps:
- Accumulated state hash (Poseidon chain of all commitments and notionals)
- Round nonce (increments each step)
- Policy hash: poseidon(min_size, min_price, position_limit), constant across all steps

At the end, the accumulator gets compressed into a final proof that goes on-chain.

---

## Smart contracts

Three contracts, deployed with Foundry (Cancun EVM, via-IR, 200 optimizer runs).

### DarkPool.sol

The main contract. Handles deposits, withdrawals, and settlement.

**Account management:**

| Function | Who | What |
|----------|-----|------|
| `deposit(token, amount)` | Anyone | Deposit ERC20 collateral |
| `withdraw(token, amount)` | Anyone | Withdraw available balance |

**Settlement (two paths):**

*Path A: single-batch Groth16 (legacy)*
```
submitBatch(batchId, auctionId, proof, publicInputs[6], matches[])
```
Verifies a Groth16 proof, then settles all matches atomically. The proof is 256 bytes (uncompressed G1, G2, G1 points). Public inputs carry match count, commitment roots, and policy params.

*Path B: IVC sessions (current)*
```
submitSession(sessionId, proof, z_0, z_n, nSteps, policyHash, matchesHash)
settleAuction(sessionId, auctionId, matches[])
```
Two-step process. First call verifies the HyperNova proof and stores a hash commitment to the matches. Second call re-derives `keccak256(abi.encode(auctionId, matches))`, checks it against the stored hash, and settles. This prevents the operator from swapping in different matches after the proof was verified.

**Match settlement (internal):**

For each match, the contract:
- Computes `notional = price * size / 1e18`
- Takes a 5 bps protocol fee from the notional
- Debits the buyer's quote token, credits their base token
- Debits the seller's base token, credits their quote token (minus fee)
- Credits the fee recipient

**Governance:**

| Function | What |
|----------|------|
| `setPolicy(minSize, minPrice, positionLimit)` | Update circuit constraints |
| `setFeeRecipient(recipient)` | Change where fees go |
| `setIvcVerifier(address)` | Point to a new IVC verifier |
| `setOperatorPubkey(pubkey, effectiveAt)` | Rotate the ECIES encryption key |
| `addOperator(op)` / `removeOperator(op)` | Manage operator accounts |
| `pause()` / `unpause()` | Emergency stop |

**Events:** `Deposit`, `Withdrawal`, `BatchSettled`, `SessionSubmitted`, `AuctionSettled`, `OperatorAdded`, `OperatorRemoved`, `OperatorPubkeyUpdated`

### VerifierProxy.sol

Sits between DarkPool and the actual verifier contracts. The point is key rotation: when you need to update the verifying key, you deploy a new verifier and point the proxy at it. DarkPool never needs redeployment.

- `setVerifier(addr)` - swap the Groth16 verifier
- `setIvcVerifier(addr)` - swap the HyperNova verifier
- `verifyProof(a, b, c, input)` - forwards to the Groth16 verifier
- `verifyIvcProof(proof, z0, zN, nSteps)` - forwards to the IVC verifier

### Groth16Verifier.sol

Immutable. The verification key is baked in at construction. Uses the EIP-197 precompile for BN254 pairing checks. The proof layout is 256 bytes: A (G1, 64 bytes), B (G2, 128 bytes in imaginary-real order for the precompile), C (G1, 64 bytes). Six public inputs.

---

## Settlement flow from the backend

The Rust crate `dp-settlement` handles submitting proofs on-chain. Two flows:

**Groth16 (legacy):** builds `SubmitBatchParams` with the 256-byte proof, 6 public inputs, and an array of `SettlementMatch` structs (prices/sizes converted to wei via `decimal * 1e18`). Estimates gas, fetches nonce, sends the tx, waits for the receipt.

**IVC (current):** two sequential transactions. First, `submitSession` with the HyperNova proof, initial/final state vectors, step count, policy hash, and matches hash. Wait for receipt. Then `settleAuction` with the session ID, auction ID, and the actual matches array. The contract re-derives the hash and rejects if it doesn't match.

UUID encoding: padded to bytes32 (16 zero bytes + 16-byte UUID).

---

## Known gap

The matches aren't directly bound to the IVC proof output. The `sessionMatchesHash` is an operator-supplied commitment, not something derived from `z_n`. The `z_n[0]` state hash accumulates over encrypted commitments, not plaintext match data. The `settleAuction` re-derivation check is the current defense. A future circuit update should expose a plaintext match hash in the public output to close this fully.
