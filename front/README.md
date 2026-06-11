# front/

Next.js 14 App Router app that hosts **both** the marketing landing
page at `/` and the trading app under `/app/...`.

Landing is already live at https://front-five-flax.vercel.app and is
**off-limits** to trading-app issues — see
[`docs/PARALLEL-WORK.md`](../docs/PARALLEL-WORK.md) for the file-scope
rules.

## Develop

```bash
cd front
npm install
npm run dev          # http://localhost:3000
```

## Scripts

| Script                 | What it does                                   |
| ---------------------- | ---------------------------------------------- |
| `npm run dev`          | Next dev server with HMR                       |
| `npm run build`        | Production build (verified in CI)              |
| `npm run start`        | Serve the production build                     |
| `npm run typecheck`    | `tsc --noEmit` against strict TS               |
| `npm run lint`         | `next lint` (next/core-web-vitals + typescript)|
| `npm run format`       | Prettier write                                 |
| `npm run format:check` | Prettier check (verified in CI)                |

CI runs `typecheck`, `lint`, `format:check`, and `build` against every
PR via the `Frontend` job in `.github/workflows/ci.yml`.

## Environment variables

The trading app reads its runtime config from
[`front/lib/config.ts`](./lib/config.ts), which validates `process.env`
with [zod](https://zod.dev) at module load. **If a required variable
is missing or malformed the app refuses to boot** (loud error in the
dev overlay / 500 in production) — there is no silent fallback.

Copy [`.env.local.example`](./.env.local.example) to `.env.local` and
edit the values. `.env.local` is git-ignored.

| Variable                              | Required when                          | Notes                                                            |
| ------------------------------------- | -------------------------------------- | ---------------------------------------------------------------- |
| `NEXT_PUBLIC_USE_MOCKS`               | always                                 | `true` / `false`. Phase 1 default is `true`.                     |
| `NEXT_PUBLIC_DARKPOOL_API_URL`        | always                                 | Base URL of `dp-api`. Dev proxies `/api/v1/*` here.              |
| `NEXT_PUBLIC_DARKPOOL_API_KEY`        | always                                 | Sent as `x-api-key` header by the SDK.                           |
| `NEXT_PUBLIC_CHAIN_ID`                | always                                 | Positive integer. Anvil default `31337`.                         |
| `NEXT_PUBLIC_OPERATOR_PUBKEY_URL`     | always                                 | ECIES pubkey endpoint (lands with C1, #81).                      |
| `NEXT_PUBLIC_DARKPOOL_ADDRESS`        | `NEXT_PUBLIC_USE_MOCKS=false`          | `0x` + 40 hex chars.                                             |
| `NEXT_PUBLIC_VERIFIER_PROXY_ADDRESS`  | `NEXT_PUBLIC_USE_MOCKS=false`          | `0x` + 40 hex chars.                                             |
| `NEXT_PUBLIC_USDC_ADDRESS`            | `NEXT_PUBLIC_USE_MOCKS=false`          | `0x` + 40 hex chars.                                             |
| `NEXT_PUBLIC_WETH_ADDRESS`            | `NEXT_PUBLIC_USE_MOCKS=false`          | `0x` + 40 hex chars.                                             |

### Per-RPC mock overrides

Phase 2 swaps the mock client for `RestClient` one RPC at a time. Each
override layers on top of `NEXT_PUBLIC_USE_MOCKS`: `true` / `1` forces the
mock impl for that RPC, `false` / `0` forces the REST impl, and unset
falls back to the global flag. Parsing lives in
[`methodOverridesFromEnv`](./lib/sdk/client.ts).

| Variable                                     | Routes              | Notes                                                                       |
| -------------------------------------------- | ------------------- | --------------------------------------------------------------------------- |
| `NEXT_PUBLIC_USE_MOCKS_ORDERBOOK`            | `getOrderBook`      | I2.4 (#93). Hits `GET /v1/orderbook` when `false`.                          |
| `NEXT_PUBLIC_USE_MOCKS_AUCTION_HISTORY`      | `getAuctionHistory` | I2.5 (#94). Hits `GET /v1/auctions` when `false`.                           |
| `NEXT_PUBLIC_USE_MOCKS_STREAM_AUCTIONS`      | `streamAuctions`    | I2.6 (#95). Keep `true` until the SSE bridge ships — REST throws otherwise. |
| `NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER`          | `placeOrder`        | I2.10 (#99). Needs the ZK prover + browser ECIES first.                     |
| `NEXT_PUBLIC_USE_MOCKS_CANCEL_ORDER`         | `cancelOrder`       | Pair with `placeOrder` so cancels target real orders.                       |
| `NEXT_PUBLIC_USE_MOCKS_GET_ORDER`            | `getOrder`          | Standalone — flip once `placeOrder` is real.                                |

### Dev proxy

`next.config.mjs` rewrites `/api/v1/:path*` →
`${NEXT_PUBLIC_DARKPOOL_API_URL}/v1/:path*` **only in `npm run dev`**.
This dodges CORS until [C2 (#82)](https://github.com/Dnreikronos/DarkPool-Exchange/issues/82)
lands real CORS on the REST router. Production builds don't rewrite —
the SDK is expected to call the backend directly.

## Structure

```
front/
├── app/
│   ├── page.tsx              # landing (/)
│   ├── layout.tsx            # root layout (fonts, dot-grid overlay)
│   ├── globals.css           # site-wide CSS (crosshair cursor, overlay)
│   └── app/                  # trading app routes — built out by F1.x
│       └── ...               # owned per docs/PARALLEL-WORK.md
├── components/
│   ├── (landing components)  # Nav, Hero, Footer, Ticker, ... — off-limits
│   └── trade/                # trading-app components — built by F1.x
├── lib/
│   ├── shaders/              # landing-only — off-limits
│   └── sdk/                  # generated TS SDK — F0.3
└── public/                   # static assets
```

The trading app builds against the design system in
[`DESIGN.md`](../DESIGN.md) (brutalist trading-terminal aesthetic, lime
accent `#D4FF00`, Bebas Neue + IBM Plex Mono, zero radius, no semantic
colors, no icons). Read it before adding any UI.
