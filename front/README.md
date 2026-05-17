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

The trading app reads its runtime config from `front/lib/config.ts`,
which is established by [F0.6 (#67)](https://github.com/Dnreikronos/DarkPool-Exchange/issues/67).
Until that lands no env vars are required.

Document each new variable here as it gets introduced.

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
