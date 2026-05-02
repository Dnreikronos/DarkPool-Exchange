# syntax=docker/dockerfile:1.7

# ---------- builder ----------
FROM rust:1.83-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        protobuf-compiler \
        pkg-config \
        libssl-dev \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release --bin darkpool-server


# ---------- runtime ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home darkpool

WORKDIR /app

COPY --from=builder /build/target/release/darkpool-server /usr/local/bin/darkpool-server

USER darkpool

ENV DARKPOOL_GRPC_ADDR=0.0.0.0:9090 \
    DARKPOOL_HTTP_ADDR=0.0.0.0:8080

EXPOSE 8080 9090

ENTRYPOINT ["/usr/local/bin/darkpool-server"]
