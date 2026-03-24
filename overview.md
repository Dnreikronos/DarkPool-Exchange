# ZK Dark Pool DEX
## Technical Architecture & Implementation Guide
> Stack: Go · Rust · Solidity · ZK Circuits (halo2 / arkworks)

---

## 1. Project Overview

ZK Dark Pool DEX is a privacy-preserving decentralized exchange where all orders are hidden from external observers until the moment of settlement. Traders encrypt their orders to the engine operator's public key and submit cryptographic proofs that their orders are valid — sufficient collateral, correct format, within position limits — without revealing the pair, price, or size to any on-chain observer.

The operator decrypts and matches orders in cleartext, but generates a ZK proof that the matching was executed correctly — no front-running, no favoritism. Privacy is guaranteed against the outside world; the operator is semi-trusted.

**Three core design goals:**
- **Pre-trade privacy:** no order data is visible on-chain before settlement. External observers see only commitments.
- **Post-trade verifiability:** every matched trade is accompanied by a ZK proof that the settlement was computed correctly.
- **Fair execution via batch auction:** orders are collected over a fixed time window and matched in periodic batch auctions, eliminating temporal advantage between participants within the same batch.

The target audience is protocols and institutions that need MEV protection and order confidentiality — hedge funds, market makers, and DeFi protocols building on top of privacy layers.

### 1.1 Why This Project Stands Out

| Dimension | Typical DEX | ZK Dark Pool DEX |
|---|---|---|
| Order visibility | Public mempool, front-runnable | Private until settlement |
| Proof system | None | ZK-SNARK per order batch |
| Matching model | Continuous on-chain (expensive, front-runnable) | Off-chain periodic batch auction |
| Price discovery | Visible order book | Clearing price revealed after each auction round |
| Settlement | Immediate per-order | Batched, gas-efficient |
| Trust model | Trustless (but transparent) | Semi-trusted operator with ZK proof of correct execution |
| Stack complexity | Solidity only | Go + Rust + Solidity + ZK |

### 1.2 Trust Model

The matching engine operator is **semi-trusted**:

- The trader encrypts the full order (pair, side, price, size) to the operator's public key before submission.
- The operator decrypts and matches orders in cleartext internally.
- The operator generates a ZK proof that the batch auction was executed correctly: the clearing price is fair, no orders were dropped or reordered maliciously, and all fills are valid.
- The operator **can** see order contents but **cannot** tamper with matching without the proof failing on-chain verification.
- Privacy is against **external observers** (other traders, on-chain watchers, MEV bots), not against the operator itself.

This mirrors how institutional dark pools work in traditional finance, where the venue operator sees order flow but is bound by regulation (and in our case, by cryptographic proof) to execute fairly.

---

## 2. Business Logic

### 2.1 Order Lifecycle

An order passes through five discrete phases before funds change hands:

1. **Commitment** — trader submits a Pedersen commitment to the order parameters and locks collateral in the escrow contract.
2. **Proof generation** — trader runs a Rust circuit locally and produces a ZK proof that the order is valid (sufficient collateral, correct format, within limits).
3. **Encrypted submission** — trader encrypts the full order to the operator's public key and submits the commitment, proof, and encrypted payload to the API gateway.
4. **Batch auction** — the operator decrypts orders, collects them into time-bounded batches (default: every 5 seconds), computes a clearing price, and matches bids and asks that cross at or through that price.
5. **Settlement** — matched pairs are batched (up to 256 per batch) and submitted on-chain with an aggregated ZK proof. The Solidity verifier checks the proof and transfers tokens atomically.

### 2.2 Matching Rules

- **Batch auction model:** orders are collected over a configurable time window (default: 5 seconds). At the end of each window, a clearing price is computed and all crossing orders execute at that single price.
- No temporal advantage within a batch — all orders in the same auction round are treated equally regardless of arrival time.
- Partial fills are supported; residual quantity carries over to the next auction round.
- Orders expire after a configurable TTL (default: 10 minutes).
- Self-match prevention: orders from the same commitment key cannot match.
- Minimum order size enforced at the circuit level, not in the engine.
- Clearing price is computed as the price that maximizes matched volume.

### 2.3 Settlement Rules

- Settlement occurs in batches of up to 256 matched pairs.
- Each batch is accompanied by an aggregated ZK proof verified by the on-chain verifier contract.
- If the proof fails, the entire batch is rejected — no partial settlement.
- Collateral is locked in the escrow contract at commitment time and released atomically at settlement.
- A 0.05% protocol fee is deducted from the taker side at settlement.

### 2.4 Price Discovery

Price emerges from the protocol itself via the batch auction mechanism:

- Each auction round produces a clearing price — the price at which maximum volume is matched.
- The history of clearing prices serves as the price feed for the market.
- No dependency on external oracles for price formation.
- The frontend displays historical clearing prices and aggregated depth (without revealing individual orders).

### 2.5 Privacy Guarantees

- An external observer cannot determine the price or size of any pending order from on-chain data.
- The matching engine operator **can** see order contents (decrypted from the encrypted payload) but is cryptographically bound to execute the auction correctly via ZK proofs.
- Post-settlement, trade amounts and the clearing price are revealed but individual orders are unlinkable to wallet addresses without additional information.

---

## 3. System Architecture

### 3.1 High-Level Components

| Layer | Language | Responsibility |
|---|---|---|
| ZK Circuit | Rust (halo2 / arkworks) | Generate & verify proofs of order validity |
| Matching Engine | Go | Batch auction logic, clearing price computation, event sourcing |
| Event Store | Go | Immutable event log (OrderPlaced, OrderMatched, BatchSubmitted, etc.) for state reconstruction and auditability |
| Proof Aggregator | Rust (CLI binary) | Combine individual proofs into a single batch proof. Called by Go via exec |
| Settlement Contract | Solidity | On-chain proof verification, token transfer, escrow |
| API Gateway | Go (gRPC + REST) | Client-facing order submission and status endpoints |
| Demo Frontend | TypeScript / Next.js | Auction history, clearing prices, aggregated depth |

### 3.2 Data Flow

```
Trader (client)
  |
  | 1. Generate ZK proof locally (Rust WASM or native CLI)
  | 2. Encrypt full order to operator's public key
  | 3. POST /order  { commitment, proof, encrypted_payload }
  v
API Gateway (Go)
  |
  | 4. Verify proof format + validate commitment
  | 5. Rate limiting, auth
  | 6. Enqueue to matching engine via gRPC
  v
Matching Engine (Go)
  |
  | 7. Decrypt order using operator private key
  | 8. Append OrderPlaced event to event store
  | 9. Collect orders into time-bounded batch (default: 5s window)
  | 10. On auction tick: compute clearing price, match crossing orders
  | 11. Append AuctionExecuted + OrderMatched events to event store
  v
Proof Aggregator (Rust CLI)
  |
  | 12. Receive matched pairs via stdin/file
  | 13. Aggregate individual ZK proofs into single batch proof
  v
Settlement Contract (Solidity / EVM)
  |
  | 14. Verify aggregated proof on-chain
  | 15. Release escrow, transfer tokens, emit Settlement event + clearing price
  v
Done — clearing price visible on-chain, individual orders remain private
```

### 3.3 Event Sourcing Model

The matching engine does not maintain mutable state. Instead, the order book is a **projection** derived from an append-only sequence of immutable events:

| Event | Description |
|---|---|
| `OrderPlaced` | New order decrypted and accepted into the current batch |
| `OrderCancelled` | Order withdrawn by the trader before auction execution |
| `AuctionExecuted` | Batch auction tick: clearing price computed |
| `OrderMatched` | Order fully or partially filled at clearing price |
| `OrderExpired` | Order TTL exceeded without fill |
| `BatchSubmitted` | Matched pairs submitted on-chain for settlement |
| `BatchConfirmed` | On-chain settlement tx confirmed |

**Recovery:** on startup, the engine replays the event log from the last compaction checkpoint to reconstruct the current order book state. A batch is only considered committed when the `BatchSubmitted` event is persisted.

**Compaction:** periodic snapshots of the projected state allow truncation of old events, keeping replay time bounded.

### 3.4 Repository Structure

```
darkpool/
├── engine/
│   ├── orderbook.go         # Order book projection from event stream
│   ├── auction.go           # Batch auction logic, clearing price computation
│   ├── events.go            # Event types and event store interface
│   ├── store.go             # Append-only event log + compaction
│   ├── orderbook_test.go    # Unit tests
│   ├── auction_test.go      # Auction logic tests
│   └── bench_test.go        # Benchmarks
├── zkproof/
│   ├── circuits/            # Rust: halo2 circuits for order validity
│   ├── prover/              # Proof generation (native + WASM target)
│   └── aggregator/          # Rust CLI binary for batch proof aggregation
├── contracts/
│   ├── DarkPool.sol         # Main escrow + settlement contract
│   ├── Verifier.sol         # Auto-generated from circuit (halo2-verifier)
│   └── test/                # Foundry tests
├── api/
│   ├── server.go            # gRPC server + REST gateway
│   ├── proto/               # Protobuf definitions
│   └── middleware/          # Rate limiting, auth
├── frontend/
│   └── ...                  # Next.js demo UI
├── docs/
│   ├── whitepaper.md        # Technical design document
│   └── benchmarks.md        # Public performance results
└── docker-compose.yml       # Local dev stack
```
