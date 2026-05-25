#!/usr/bin/env node
//
// E2E test: connect to the SSE auction stream and consume 3 events.
//
// Usage:
//   1. Start the server with a short auction interval and pre-registered pair:
//      cargo run -p dp-api -- --auction-interval 2s --pair ETH/USDC
//   2. Place crossing orders (buy + sell at same price) to trigger matches.
//   3. Run this script:
//      node tests/e2e/sse_stream.mjs
//
// Environment:
//   DARKPOOL_REST_URL  (default: http://127.0.0.1:8080)
//   DARKPOOL_API_KEY   (default: empty — works when server has no keys configured)

const BASE = process.env.DARKPOOL_REST_URL ?? "http://127.0.0.1:8080";
const API_KEY = process.env.DARKPOOL_API_KEY ?? "";

const WANT = 3;
const TIMEOUT_MS = 120_000;

const params = new URLSearchParams();
params.set("pair", "ETH/USDC");
if (API_KEY) params.set("apiKey", API_KEY);

const url = `${BASE}/v1/auctions/stream?${params}`;
console.log(`Connecting to ${url}`);
console.log(`Waiting for ${WANT} auction events…\n`);

const events = [];
const es = new EventSource(url);

es.addEventListener("auction", (e) => {
  const data = JSON.parse(e.data);
  events.push(data);
  console.log(`[${events.length}/${WANT}]`, data);
  if (events.length >= WANT) {
    es.close();
    validate(events);
  }
});

es.addEventListener("error", (e) => {
  const data = e.data ? JSON.parse(e.data) : null;
  if (data?.lagged) {
    console.warn(`Lagged: missed ${data.lagged} events`);
  }
});

es.onerror = (e) => {
  console.error("EventSource error (will auto-reconnect):", e.type);
};

setTimeout(() => {
  es.close();
  console.error(
    `\nTimed out after ${TIMEOUT_MS / 1000}s: received ${events.length}/${WANT} events`,
  );
  process.exit(1);
}, TIMEOUT_MS);

function validate(events) {
  const fields = [
    "auctionId",
    "pair",
    "clearingPrice",
    "matchedVolume",
    "matchCount",
    "timestampUnix",
  ];
  for (const ev of events) {
    for (const f of fields) {
      assert(ev[f] !== undefined && ev[f] !== null, `missing field: ${f}`);
    }
    assert(typeof ev.matchCount === "number", "matchCount must be number");
  }
  console.log(`\nOK: received and validated ${events.length} auction events`);
  process.exit(0);
}

function assert(cond, msg) {
  if (!cond) {
    console.error("FAIL:", msg);
    process.exit(1);
  }
}
