# ADR 0004 — SSE bridge for auction streaming

- **Status:** Accepted
- **Date:** 2026-05-25
- **Issue:** [#83](https://github.com/Dnreikronos/DarkPool-Exchange/issues/83)

## Context

The gRPC `StreamAuctions` RPC works for native clients but browsers
cannot speak raw gRPC (HTTP/2 binary framing). The frontend TypeScript
SDK is REST-based and currently throws `UNIMPLEMENTED` for streaming.
Real-time auction settlement events need a browser-compatible transport.

## Decision

Add a `GET /v1/auctions/stream` SSE endpoint on the existing Axum REST
listener that subscribes to the same `broadcast::Sender<AuctionNotification>`
used by the gRPC stream.

### Why SSE over gRPC-web (tonic-web)

1. **No client library.** SSE works with the browser-native `EventSource`
   API. grpc-web requires a dedicated client library plus proto codegen
   targeting the web transport.
2. **JSON everywhere.** The REST surface already serialises to camelCase
   JSON. SSE `data:` lines carry the same shape — no binary framing.
3. **Proxy transparency.** SSE is plain HTTP/1.1 chunked transfer. It
   transits every proxy, ALB, and CDN without special configuration.
4. **No new dependency.** `axum::response::sse` ships in axum 0.7's
   default feature set.

### Wire format

Each auction settlement is an SSE event:

    event: auction
    data: {"auctionId":"...","pair":"ETH/USDC","clearingPrice":"2001.5","matchedVolume":"12.0","matchCount":4,"timestampUnix":"1716508800"}

Broadcast channel overflow surfaces as:

    event: error
    data: {"lagged":3}

A 15-second keepalive comment prevents proxy idle-timeout disconnects:

    : keepalive

### Auth

`EventSource` cannot set custom headers. The `auth_axum_mw` middleware
gains a query-parameter fallback: when the `x-api-key` header is absent,
it checks `?apiKey=<token>` before rejecting. The EventSource URL
becomes `/v1/auctions/stream?apiKey=<token>`.

### Pair filtering

Optional `?pair=ETH/USDC` query parameter. Validated against the pair
registry — unknown pairs return 404. Empty = all pairs.

## Consequences

- Broadcast channel capacity (64) bounds how many un-consumed events a
  slow SSE client can buffer before lag. Identical to the gRPC stream.
- SSE is unidirectional. Bidirectional streaming (if ever needed) would
  require WebSocket alongside SSE, not replacing it.
- The `apiKey` query parameter appears in server access logs. Security
  posture is unchanged since the key already travels in cleartext over
  HTTPS; log scrubbing is a separate operational concern.
