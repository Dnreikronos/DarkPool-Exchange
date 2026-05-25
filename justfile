# DarkPool Exchange - task runner
# Usage: `just <recipe>`  (run `just` or `just help` to list)

set shell := ["bash", "-cu"]
set dotenv-load := true

# Vars (override on CLI: `just CRATE=dp-api test-crate`)
CRATE          := env_var_or_default("CRATE", "dp-api")
COMPOSE        := env_var_or_default("COMPOSE", "docker compose")
RUST_LOG       := env_var_or_default("RUST_LOG", "info,dp_event=debug,dp_engine=info")
ZK_AGGREGATOR  := "/usr/local/bin/dp-aggregator"
ZK_KEYS        := "/keys"

# Default: list recipes
default:
    @just --list

help: default

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

# Build entire workspace (release)
build:
    cargo build --release --workspace

# Debug build of workspace
build-debug:
    cargo build --workspace

# Build solidity contracts via forge
build-contracts:
    cd contracts && forge build

# Build everything (rust + contracts)
build-all: build build-contracts

# Build frontend (Next.js)
build-front:
    cd front && pnpm install --frozen-lockfile && pnpm build

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------

# Workspace tests
test:
    cargo test --workspace

# Tests for a single crate (CRATE=dp-foo just test-crate)
test-crate:
    cargo test -p {{CRATE}}

# Run ignored / heavy tests too
test-all:
    cargo test --workspace -- --include-ignored

# Forge contract tests
test-contracts:
    cd contracts && forge test -vv

# Everything (rust + contracts)
test-full: test test-contracts

# Coverage via tarpaulin (provisions postgres internally — see CI script)
coverage:
    cargo tarpaulin --workspace --skip prove_batch_within_budget --include-ignored --out Html --output-dir target/tarpaulin

# ---------------------------------------------------------------------------
# Lint / format / check
# ---------------------------------------------------------------------------

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

check:
    cargo check --workspace --all-targets

# fmt-check + clippy + check (pre-push gate)
ci-local: fmt-check clippy check test

# ---------------------------------------------------------------------------
# Docker / services
# ---------------------------------------------------------------------------

# Bring up the standard stack (postgres + anvil + deployer + server)
up:
    {{COMPOSE}} up -d --build

# Bring up stack with the ZK profile (aggregator + keygen one-shot)
up-zk:
    DARKPOOL_AGGREGATOR_BIN={{ZK_AGGREGATOR}} \
    DARKPOOL_ZK_PROVING_KEY={{ZK_KEYS}} \
    {{COMPOSE}} --profile zk up -d --build

# Bring up the obs stack (default services + prometheus/grafana/jaeger; server pushes OTLP to jaeger)
up-obs:
    OTEL_EXPORTER_OTLP_ENDPOINT=${OTEL_EXPORTER_OTLP_ENDPOINT:-http://jaeger:4317} \
    {{COMPOSE}} --profile obs up -d --build

# Tear down only the obs services (leaves postgres/anvil/deployer/darkpool-server up)
down-obs:
    {{COMPOSE}} --profile obs rm -sf prometheus grafana jaeger

# Tail logs for the obs services
logs-obs:
    {{COMPOSE}} --profile obs logs -f prometheus grafana jaeger

# Foreground (logs streamed)
up-fg:
    {{COMPOSE}} up --build

down:
    {{COMPOSE}} down

# Stop + remove volumes (DESTRUCTIVE: drops postgres + zk-keys)
clean:
    {{COMPOSE}} down -v

restart: down up

# Tail logs for one service: `just logs darkpool-server`
logs SVC="darkpool-server":
    {{COMPOSE}} logs -f --tail=200 {{SVC}}

ps:
    {{COMPOSE}} ps

# Open a shell in a running service: `just sh postgres`
sh SVC="darkpool-server":
    {{COMPOSE}} exec {{SVC}} sh

# ---------------------------------------------------------------------------
# Backend services (run locally, outside docker)
# ---------------------------------------------------------------------------

# Run the darkpool API/gRPC server with debug logging
run-server:
    RUST_LOG={{RUST_LOG}} cargo run -p dp-api --bin darkpool-server

# Run the aggregator binary
run-aggregator:
    RUST_LOG={{RUST_LOG}} cargo run -p dp-zk-cli --bin dp-aggregator

# Run the ZK CLI
run-zk-cli *ARGS:
    cargo run -p dp-zk-cli --bin dp-zk-cli -- {{ARGS}}

# Generate proving keys locally (writes to ./keys)
keygen BATCH="8" SEED="42":
    cargo run -p dp-zk-cli --bin dp-zk-keygen -- --out ./keys --batch-size {{BATCH}} --seed {{SEED}}

# Local anvil dev chain
anvil:
    anvil --host 0.0.0.0 --chain-id 31337 --block-time 1

# Deploy contracts to a local anvil
deploy RPC="http://127.0.0.1:8545":
    mkdir -p contracts/deployments
    cd contracts && GIT_SHA=$(git rev-parse --short HEAD) forge script script/Deploy.s.sol:DeployScript --rpc-url {{RPC}} --broadcast

# Deploy contracts to a remote chain (Alchemy RPC from .env)
deploy-remote:
    @test -n "${RPC_URL:-}" || { echo "ERROR: RPC_URL not set in .env"; exit 1; }
    @test -n "${PRIVATE_KEY:-}" || { echo "ERROR: PRIVATE_KEY not set in .env"; exit 1; }
    mkdir -p contracts/deployments
    cd contracts && GIT_SHA=$(git rev-parse --short HEAD) forge script script/Deploy.s.sol:DeployScript \
        --rpc-url "$RPC_URL" \
        --broadcast \
        --verify

# ---------------------------------------------------------------------------
# Database
# ---------------------------------------------------------------------------

# psql shell into the dockerised postgres
psql:
    {{COMPOSE}} exec postgres psql -U "${POSTGRES_USER:-darkpool}" -d "${POSTGRES_DB:-darkpool}"

# Drop + recreate the postgres volume (DESTRUCTIVE)
db-reset:
    {{COMPOSE}} stop postgres
    {{COMPOSE}} rm -f postgres
    docker volume rm darkpool-exchange_postgres-data || true
    {{COMPOSE}} up -d postgres

# ---------------------------------------------------------------------------
# Frontend
# ---------------------------------------------------------------------------

front-dev:
    cd front && pnpm install && pnpm dev

front-lint:
    cd front && pnpm lint

# ---------------------------------------------------------------------------
# Housekeeping
# ---------------------------------------------------------------------------

# Cargo clean + forge clean
nuke:
    cargo clean
    cd contracts && forge clean

# Show env required by the stack
env:
    @[ -f .env ] && cat .env || cat .env.example
