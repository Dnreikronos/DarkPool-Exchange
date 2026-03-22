# ZK Dark Pool DEX
## Technical Architecture & Implementation Guide
> Stack: Go · Rust · Solidity · ZK Circuits (halo2 / arkworks)

---

## 1. Project Overview

ZK Dark Pool DEX is a privacy-preserving decentralized exchange where all orders are completely hidden until the moment of settlement. Traders submit cryptographic proofs that their orders are valid — sufficient collateral, correct format, within position limits — without revealing the pair, price, or size to any counterparty or on-chain observer.

**Three core design goals:**
- **Pre-trade privacy:** no order data is visible on-chain before matching.
- **Post-trade verifiability:** every matched trade is accompanied by a ZK proof that the settlement was computed correctly.
- **High-throughput matching:** a Go-based off-chain matching engine handles up to 100,000 orders/sec with p99 latency below 1ms.

The target audience is protocols and institutions that need MEV protection and order confidentiality — hedge funds, market makers, and DeFi protocols building on top of privacy layers.

### 1.1 Why This Project Stands Out

| Dimension | Typical DEX | ZK Dark Pool DEX |
|---|---|---|
| Order visibility | Public mempool, front-runnable | Private until settlement |
| Proof system | None | ZK-SNARK per order batch |
| Matching engine | On-chain (expensive) | Off-chain Go engine, O(log n) |
| Settlement | Immediate per-order | Batched, gas-efficient |
| Stack complexity | Solidity only | Go + Rust + Solidity + ZK |

---

## 2. Business Logic

### 2.1 Order Lifecycle

An order passes through four discrete phases before funds change hands:

1. **Commitment** — trader submits a Pedersen commitment to the order parameters.
2. **Proof generation** — trader runs a Rust circuit locally and produces a ZK proof of validity.
3. **Matching** — the Go engine matches bids and asks using price-time priority, operating only on commitments.
4. **Settlement** — a batch of matched pairs is submitted on-chain with aggregated proofs; the Solidity verifier checks each proof and transfers tokens atomically.

### 2.2 Matching Rules

- Price-time priority (FIFO within the same price level).
- Partial fills are supported; residual quantity remains in the book.
- Orders expire after a configurable TTL (default: 10 minutes).
- Self-match prevention: orders from the same commitment key cannot match.
- Minimum order size enforced at the circuit level, not in the engine.

### 2.3 Settlement Rules

- Settlement occurs in batches of up to 256 matched pairs.
- Each batch is accompanied by an aggregated proof verified by the on-chain verifier contract.
- If the proof fails, the entire batch is rejected — no partial settlement.
- Collateral is locked in the escrow contract at commitment time and released atomically at settlement.
- A 0.05% protocol fee is deducted from the taker side at settlement.

### 2.4 Privacy Guarantees

- An external observer cannot determine the price or size of any pending order from on-chain data.
- The matching engine operator cannot learn order contents — it only sees commitments and proof validity bits.
- Post-settlement, trade amounts are revealed but are unlinkable to wallet addresses without additional information.

---

## 3. System Architecture

### 3.1 High-Level Components

| Layer | Language | Responsibility |
|---|---|---|
| ZK Circuit | Rust (halo2 / arkworks) | Generate & verify proofs of order validity |
| Matching Engine | Go | In-memory order book, price-time matching, WAL |
| Settlement Contract | Solidity | On-chain proof verification, token transfer, escrow |
| API Gateway | Go (gRPC + REST) | Client-facing order submission and status endpoints |
| Demo Frontend | TypeScript / Next.js | Show anonymized order book depth and trade history |

### 3.2 Data Flow
```
Trader (client)
  |
  | 1. Generate ZK proof locally (Rust WASM or native CLI)
  | 2. POST /order  { commitment, proof, encrypted_payload }
  v
API Gateway (Go)
  |
  | 3. Verify proof format (not validity — that happens on-chain)
  | 4. Enqueue to matching engine via gRPC
  v
Matching Engine (Go)
  |
  | 5. Insert into in-memory order book (btree, price-time priority)
  | 6. Run matching loop — emit MatchedPair events
  | 7. Append to WAL for crash recovery
  v
Batch Aggregator (Go)
  |
  | 8. Collect up to 256 matched pairs
  | 9. Call Rust aggregator to combine proofs
  v
Settlement Contract (Solidity / EVM)
  |
  | 10. Verify aggregated proof on-chain
  | 11. Release escrow, transfer tokens, emit Settlement event
  v
Done — trade visible on-chain as settled amount only
```

### 3.3 Repository Structure
```
darkpool/
├── engine/
│   ├── orderbook.go         # Core order book: btree + price-time priority
│   ├── matcher.go           # Matching loop, partial fills
│   ├── wal.go               # Write-ahead log for crash recovery
│   ├── orderbook_test.go    # Unit tests
│   └── bench_test.go        # Benchmarks (target: 100k orders/s)
├── zkproof/
│   ├── circuits/            # Rust: halo2 circuits for order validity
│   ├── prover/              # Proof generation (native + WASM target)
│   └── aggregator/          # Batch proof aggregation
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
