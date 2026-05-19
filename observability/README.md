# Observability

The operator process exposes three sources of telemetry. All three are
optional in dev and expected to be wired in prod.

## Endpoints

The REST listener (`DARKPOOL_HTTP_ADDR`, default `:8080`) carries an
unauthenticated ops sub-router alongside the auth-gated trading
surface.

| Path       | Method | Purpose                                                 |
| ---------- | ------ | ------------------------------------------------------- |
| `/healthz` | `GET`  | Liveness. Returns `200 {"status":"ok"}` while the process is running. |
| `/readyz`  | `GET`  | Readiness. `200` once every probe (event store, aggregator binary) is healthy; `503 {"status":"not_ready","failed":<probe_name>}` otherwise. The detailed failure reason is logged server-side (the endpoint is unauthenticated, so it must not echo internals). |
| `/metrics` | `GET`  | Prometheus text exposition (`v0.0.4`). Counter and gauge families are force-registered at startup so they appear before the first auction tick. Histogram families (`darkpool_auction_duration_seconds`, `darkpool_batch_submission_duration_seconds`) only appear after their first observation — see the histogram note in the quick-start section. |

## Environment variables

| Var                            | Default                                | Effect                                                                                                                                |
| ------------------------------ | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `RUST_LOG`                     | `info`                                 | Standard `tracing-subscriber` filter (e.g. `info,dp_engine=debug`).                                                                   |
| `DP_LOG_FORMAT`                | auto (json when stdout is not a TTY)   | `json` (Loki / Vector / Datadog friendly) or `text` (developer-readable).                                                             |
| `OTEL_EXPORTER_OTLP_ENDPOINT`  | unset                                  | When set, install an OTLP/gRPC span exporter and ship spans to the configured collector. Empty / unset = no OTel layer, zero overhead. |
| `OTEL_SERVICE_NAME`            | `dp-api`                               | Resource attribute on every exported span.                                                                                            |
| `DP_ENVIRONMENT`               | `development`                          | Sets the `deployment.environment` OTel resource attribute so spans from local / staging / prod are filterable in the collector. Falls back to `DEPLOYMENT_ENVIRONMENT` if unset.  |

Defaults above are the binary's own fallbacks. `docker-compose.yml` overrides
some of them for the obs stack (`OTEL_SERVICE_NAME=darkpool-api`,
`DP_ENVIRONMENT=local`); see the quick-start section.

## Metric catalogue

| Name                                              | Type      | Labels    | Description                                                  |
| ------------------------------------------------- | --------- | --------- | ------------------------------------------------------------ |
| `darkpool_auctions_total`                         | counter   |           | Auctions that produced ≥1 match.                            |
| `darkpool_auction_duration_seconds`               | histogram |           | `dp_auction::run` wall-clock per pair, per tick.            |
| `darkpool_orders_placed_total`                    | counter   | `side`    | Orders successfully inserted into the book.                  |
| `darkpool_orders_matched_total`                   | counter   |           | Matches (bid/ask pairs) produced in successful auctions. Each match is one bid + one ask, so per-order rate = 2 × match rate. |
| `darkpool_orders_expired_total`                   | counter   |           | Orders removed because TTL elapsed.                          |
| `darkpool_clearing_price`                         | gauge     | `pair`    | Latest clearing price (best-effort, lossy past `f64`).      |
| `darkpool_batch_submission_duration_seconds`      | histogram |           | `submitter.submit` round-trip on the success path.          |
| `darkpool_settlement_confirmations_total`         | counter   |           | `BatchSettled` events handled by `BatchSink::on_batch_settled`. |
| `darkpool_active_orders`                          | gauge     |           | Orders resting in the book after each tick.                  |
| `darkpool_event_log_size_bytes`                   | gauge     |           | Polled every 30 s. Backend-specific: `pg_total_relation_size('events')` for postgres, `metadata().len()` for the file store, `0` for in-memory. |

## Quick start (local stack)

The repo ships a complete Prometheus + Grafana + Jaeger stack wired
to `darkpool-server`, gated behind a docker compose profile so the
default `just up` is unaffected.

```bash
just up-obs
```

That brings up `postgres`, `anvil`, `deployer`, `darkpool-server`,
`prometheus`, `grafana`, and `jaeger`. `OTEL_EXPORTER_OTLP_ENDPOINT`
is exported in the recipe so the server ships spans to the in-network
Jaeger at `http://jaeger:4317`.

Smoke test (under a minute):

```bash
# 1. Server is healthy + emitting metric families
curl -fsS http://localhost:8080/healthz
curl -fsS http://localhost:8080/metrics | grep '^darkpool_' | head

# 2. Prometheus has the darkpool target marked "up"
curl -fsS http://localhost:9091/api/v1/targets \
  | jq '.data.activeTargets[] | {labels, health}'

# 3. Generate a handful of HTTP spans
for i in $(seq 1 5); do curl -s http://localhost:8080/healthz > /dev/null; done
```

Open in a browser:

- Grafana — http://localhost:3001 (anonymous admin) → Dashboards → **DarkPool Operator**. Host port `3001` avoids collision with `front/` Next.js dev (3000); override with `GRAFANA_PORT`.
- Jaeger — http://localhost:16686 → Service `darkpool-api` → Find Traces

Tear down the obs services only (leaves the trading stack running):

```bash
just down-obs
```

Tail the obs logs:

```bash
just logs-obs
```

Notes:
- Histograms (`darkpool_auction_duration_seconds`,
  `darkpool_batch_submission_duration_seconds`) only register after the
  first observation. With no traffic the dashboard's percentile panels
  stay empty — counters and gauges should still render.
- Dashboard rate panels use a 5-minute window (`rate(...[5m])`). A
  sub-5m smoke run will show flat lines even with traffic; let the stack
  run ≥5 minutes before judging rate panels.
- Grafana runs with anonymous admin access. Local dev only — do not
  copy this config into a deployed environment.
- Prometheus is published on host port `9091` (not `9090`) to avoid
  colliding with the gRPC listener.

## Local development

### Scrape from Prometheus

```yaml
# prometheus.yml
scrape_configs:
  - job_name: darkpool
    metrics_path: /metrics
    static_configs:
      - targets: ["localhost:8080"]
```

### Import the Grafana dashboard

`observability/grafana/darkpool.json` is a self-contained Grafana 10+
dashboard. Either copy-paste via Dashboards → Import → Upload, or wire
it via provisioning:

```yaml
# /etc/grafana/provisioning/dashboards/darkpool.yaml
apiVersion: 1
providers:
  - name: darkpool
    folder: DarkPool
    type: file
    options:
      path: /var/lib/grafana/dashboards
      foldersFromFilesStructure: false
```

### Trace through a single auction with Jaeger

```bash
docker run --rm -p 16686:16686 -p 4317:4317 jaegertracing/all-in-one:latest

OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
DP_LOG_FORMAT=text \
RUST_LOG=info,dp_engine=debug \
cargo run -p dp-api
```

Place an order, wait for the next auction tick, then open
`http://localhost:16686`. The span tree shows the request span
(`http`), nested into engine spans for order persistence and the
auction loop.

## Correlation IDs

Every response carries an `x-request-id` header. If the client supplies
one, it's echoed back unchanged. Otherwise a UUIDv4 is generated.

In JSON logs the same value appears as the `request_id` field of the
`http` span (and of any descendant span), so a log query like
`request_id=abc-…` returns the full lifecycle of a single request.
