# ZK Dark Pool DEX

A decentralized exchange where orders stay private until settlement. Traders encrypt orders to the operator and prove validity using zero-knowledge proofs, without revealing the pair, price, or size to any external observer.

Go · Rust · Solidity · ZK Circuits (halo2 / arkworks)

https://front-five-flax.vercel.app/

---

## Why this exists

On a normal DEX, your orders sit in a public mempool. Anyone can see them, front-run them, sandwich them. This project takes a different approach: orders are encrypted to the engine operator and matched in periodic batch auctions. The operator proves via ZK that every auction was executed correctly. Settlement happens in batches with aggregated ZK proofs verified on-chain.

Three things we care about:

1. Orders are invisible to external observers before settlement.
2. Every matched trade comes with a ZK proof that the batch auction was computed correctly.
3. Price emerges from the protocol itself — each auction round produces a clearing price with no oracle dependency.

| | Typical DEX | This project |
|---|---|---|
| Order visibility | Public mempool, front-runnable | Private until settlement |
| Proof system | None | ZK-SNARK per order batch |
| Matching model | Continuous on-chain (expensive) | Off-chain periodic batch auction |
| Price discovery | Visible order book | Clearing price after each auction round |
| Settlement | Immediate per-order | Batched, gas-efficient |
| Trust model | Trustless (but transparent) | Semi-trusted operator + ZK proof of correct execution |
| Stack | Solidity only | Go + Rust + Solidity + ZK |

---

## Architecture

```mermaid
flowchart TB
    subgraph Client["Trader (Client)"]
        A1["Generate ZK proof locally\n(Rust WASM / native CLI)"]
        A2["Encrypt order to\noperator public key"]
    end

    subgraph API["API Gateway (Go)"]
        B["gRPC + REST Server"]
        B1["Verify proof format\n+ validate commitment"]
        B2["Rate limiting / Auth"]
    end

    subgraph Engine["Matching Engine (Go)"]
        C["Decrypt order\n(operator private key)"]
        D["Collect orders into\ntime-bounded batch"]
        D2["Compute clearing price\n+ match crossing orders"]
        ES["Event Store\n(append-only log)"]
    end

    subgraph Aggregator["Proof Aggregator (Rust CLI)"]
        F["Receive matched pairs\nvia stdin/file"]
        G["Aggregate ZK proofs\ninto single batch proof"]
    end

    subgraph Contracts["Settlement Layer (EVM)"]
        H["DarkPool.sol\nEscrow + Settlement"]
        I["Verifier.sol\nOn-chain proof verification"]
    end

    subgraph Frontend["Demo Frontend (Next.js)"]
        J["Auction history\nclearing prices\naggregated depth"]
    end

    A1 --> A2
    A2 -- "POST /order\n{commitment, proof, encrypted_payload}" --> B
    B --> B1 --> B2
    B2 -- "gRPC" --> C
    C --> D
    D -- "Every N seconds\n(batch auction tick)" --> D2
    D2 -- "OrderMatched events" --> ES
    D -- "OrderPlaced events" --> ES
    D2 -- "Matched pairs" --> F
    F --> G
    G -- "Aggregated proof + batch" --> H
    H --> I
    I -- "Verify → Release escrow\n→ Transfer tokens" --> K["Settlement Event\n+ Clearing Price"]

    B2 -. "WebSocket\n(auction results)" .-> J
    K -. "On-chain events" .-> J
```

---

## Order lifecycle

1. Trader submits a Pedersen commitment to the order parameters and locks collateral in escrow.
2. Trader runs a Rust circuit locally, gets back a ZK proof that the order is valid.
3. Trader encrypts the full order to the operator's public key and submits commitment + proof + encrypted payload.
4. The operator decrypts, collects orders into a time-bounded batch (default: 5s), and runs a batch auction — computing a clearing price and matching all crossing orders.
5. Matched pairs are batched (up to 256) and sent on-chain with an aggregated ZK proof. The Solidity verifier checks the proof and transfers tokens atomically.

---

## Rules

### Matching

- Periodic batch auction (default: every 5 seconds). All orders in the same round are treated equally — no temporal advantage.
- Clearing price computed as the price that maximizes matched volume.
- Partial fills are supported. Residual quantity carries over to the next auction round.
- Orders expire after a configurable TTL (default: 10 min).
- Orders from the same commitment key cannot match each other.
- Minimum order size is enforced at the circuit level, not in the engine.

### Settlement

- Batches hold up to 256 matched pairs.
- If the aggregated proof fails verification, the entire batch is rejected. No partial settlement.
- Collateral is locked in escrow at commitment time and released atomically at settlement.
- 0.05% protocol fee is taken from the taker side.

### Trust model & privacy

- Nobody outside the operator can determine the price or size of a pending order from on-chain data.
- The matching engine operator **can** see decrypted order contents but is cryptographically bound to execute auctions correctly via ZK proofs. This mirrors institutional dark pools in TradFi.
- After settlement, the clearing price and trade amounts become visible but individual orders are unlinkable to wallet addresses without additional info.

---

## Components

| Layer | Language | What it does |
|---|---|---|
| ZK Circuit | Rust (halo2 / arkworks) | Generates and verifies proofs of order validity |
| Matching Engine | Go | Batch auction logic, clearing price computation, event sourcing |
| Event Store | Go | Append-only event log for state reconstruction and auditability |
| Proof Aggregator | Rust (CLI binary) | Combines individual proofs into a single batch proof |
| Settlement Contract | Solidity | On-chain proof verification, token transfers, escrow |
| API Gateway | Go (gRPC + REST) | Order submission and status endpoints |
| Demo Frontend | TypeScript / Next.js | Auction history, clearing prices, aggregated depth |

---

## Project structure

```
darkpool/
├── engine/
│   ├── consts/
│   │   └── consts.go            # Shared enums: Side, EventType
│   ├── model/
│   │   └── model.go             # Domain types: Order, Fill
│   ├── event/
│   │   ├── event.go             # Event struct, payloads, Store interface
│   │   └── store.go             # Append-only event log (in-memory impl)
│   ├── orderbook.go             # Order book projection from event stream
│   ├── auction.go               # Batch auction logic, clearing price computation
│   ├── orderbook_test.go        # Order book unit tests
│   └── auction_test.go          # Auction logic tests
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

---

## Who this is for

Hedge funds, market makers, and DeFi protocols that need MEV protection and don't want their order flow visible to the world.
