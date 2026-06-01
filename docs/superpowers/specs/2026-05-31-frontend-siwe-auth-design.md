# Frontend SIWE auth (issue #148) — design

**Status:** approved (design)
**Issue:** [#148](https://github.com/Dnreikronos/DarkPool-Exchange/issues/148) — `[Auth] Frontend SIWE flow: wallet signing, session management, SDK integration`
**Backend dependency:** #147 (already merged — `/v1/auth/{nonce,verify}` live behind `DARKPOOL_SIWE_ENABLED`)

## Goal

After the user connects their wallet, they sign an EIP-4361 (SIWE) message. The
frontend exchanges the signature for a session JWT from the backend and sends it
as `Authorization: Bearer <token>` on every subsequent API call, replacing the
static `x-api-key` for per-user auth. No separate registration step. SIWE is
feature-flagged off by default so mocks and non-SIWE deployments keep working.

SIWE does **not** touch the ZK layer: the circuit uses
`trader_id = poseidon(commitment_key)`, not Ethereum addresses. The token is
purely API auth.

## Pinned backend contract (verified against `crates/dp-api`)

The flow is already fully implemented server-side. Exact shapes:

- `GET /v1/auth/nonce` → `200 {"nonce": "<17-char alphanumeric>"}`. Nonce is
  server-stored, **single-use**, 300s TTL, cap 10k (429 when full). Only mounted
  when `DARKPOOL_SIWE_ENABLED=true`.
- `POST /v1/auth/verify` body `{"message": "<full EIP-4361 string>",
  "signature": "0x<65-byte hex>"}` → `200 {"token": "<HS256 JWT>",
  "expires_at": <unix u64 seconds>, "address": "0x<lowercase>"}`.
  - **Field is `expires_at`, not `expires_at_unix`** (the issue body is wrong; we
    honor the code).
- Subsequent requests: `Authorization: Bearer <jwt>`. Middleware checks Bearer
  **first**, falls back to `x-api-key` — **both coexist**.
- Server validates: `domain` (only if `DARKPOOL_SIWE_DOMAIN` set — exact string
  equality vs RFC-3986 authority `host[:port]`, no scheme), `chainId` (only if
  `DARKPOOL_CHAIN_ID > 0`), `expirationTime`/`notBefore` if present, nonce
  (must be live in store), EIP-191 signature. Returned `address` is lowercase.
- No refresh endpoint, no revocation — logout is client-side (drop token);
  default TTL 24h.
- SIWE-disabled deployment: nonce/verify return 404 and Bearer returns 401
  "bearer authentication not enabled".

The full EIP-4361 string is built **client-side** and sent verbatim as `message`;
it must be byte-identical to what the wallet signs. The `nonce` inside it must be
the server-issued one (never a client-generated nonce).

## Architecture — three isolated seams

### 1. Session module — `front/lib/wallet/session.ts`

Framework-agnostic source of truth. Holds `{ token, expiresAt, address } | null`,
mirrored to `localStorage['dp:session-token']` (the `dp:` prefix means the
existing `clearPerTraderLocalStorage()` wipes it on disconnect/account-switch).

- `getSessionToken(): string | null` — token only if present **and**
  `expiresAt > now` (proactive expiry). **This is what `RestClient` reads
  per-request.**
- `getSession()`, `setSession(s)`, `clearSession()`, `loadSession()` (hydrate
  from localStorage on first read, dropping if expired).
- `subscribe(cb)` / `getSnapshot()` for `useSyncExternalStore` (same shape as the
  existing `walletStore`).
- Module-level in-flight promise so concurrent `signIn()` calls coalesce.
- Persistence behind an injectable `StorageLike` seam (matches
  `_components/onboarding/storage.ts`) for clean tests.

### 2. SIWE flow — `front/lib/wallet/use-siwe-auth.ts` + pure helpers

- `siwe-api.ts` — `fetchNonce(apiUrl, fetchImpl?)` → `{nonce}`;
  `verifySiwe(apiUrl, {message, signature}, fetchImpl?)` →
  `{token, expiresAt, address}` (parses backend `expires_at`). Fetch injectable.
  Auth routes are public; send only `accept`/`content-type`.
- `siwe-message.ts` — pure builder using **viem `createSiweMessage`** (viem
  2.50.4 is installed; `viem/siwe` subpath). Inputs: `address` (checksummed),
  `chainId`, `domain = window.location.host`, `uri = window.location.origin`,
  server `nonce`, `version: '1'`, a friendly `statement`. Returns the EIP-4361
  string. **No new npm dependency** (avoids the npm-11 lockfile hazard).
- `siwe-action.ts` — pure `computeSiweAction(prevAddr, status, nextAddr)` →
  `{ kind: 'sign-in', address } | { kind: 'sign-out' } | { kind: 'noop' }`,
  mirroring the tested `computeBridgeAction` reducer. Auto-sign fires only on an
  address **transition** (connect / switch), never on a same-address
  session-clear — this is what keeps a 401 from auto-popping the wallet.
- `useSiweAuth(opts?: { autoSignIn?: boolean })` — calls wagmi `useAccount`,
  `useChainId`, `useSignMessage`; reads session via `useSyncExternalStore`.
  Exposes `{ isAuthenticated, isAuthenticating, address, error, signIn, signOut }`.
  `signIn()`: nonce → message → `signMessageAsync({ message })` → verify →
  `setSession`. User-rejection caught into `error` (retryable); **no auto-retry
  loop**. Inert when `!config.siweEnabled || config.useMocks`
  (`isAuthenticated` then mirrors wallet connection so authed UI isn't gated).

### 3. SDK token injection — `front/lib/sdk/client.ts`

- Add `getToken?: () => string | null` and `onUnauthenticated?: () => void` to
  `RestClientOptions` and `CreateDarkPoolClientOptions`, threaded through
  `makeRest()`.
- In `requestJson`: keep `x-api-key` always; **add `authorization: 'Bearer ' +
  token` when `getToken()` is non-null**. On HTTP 401, call `onUnauthenticated()`
  before throwing the existing `DarkPoolError(UNAUTHENTICATED)`.
- `provider.tsx` `getDefaultClient()` passes
  `getToken: config.siweEnabled ? getSessionToken : undefined` and
  `onUnauthenticated: clearSession`. The getter reads the live module session per
  request, so the module-scoped singleton client needs **no rebuild** on
  login/logout.

## Lifecycle wiring — `front/lib/wallet/SiweAuthBridge.tsx`

Render-null component mounted once in `WalletProviders` (sibling to
`WagmiWalletBridge`), running `useSiweAuth({ autoSignIn: true })`. Its effect uses
`computeSiweAction` to auto-sign on connect/switch and clear session on
disconnect/switch. Because auto-sign keys on address transitions, a 401-induced
`clearSession` (same address still connected) does **not** re-popup; the UI sees
`isAuthenticated=false` and shows a non-blocking "sign in again" prompt that calls
`signIn()` on click.

## Config — `front/lib/config.ts` + `front/.env.local.example`

`NEXT_PUBLIC_SIWE_ENABLED` as an **optional boolean defaulting to false** (so
existing `.env.local` files don't crash boot at module load). Added to `rawEnv`,
`baseSchema`, and both `MockConfig`/`LiveConfig` branches as `siweEnabled`.
`NEXT_PUBLIC_DARKPOOL_API_KEY` stays required for this PR (documented as
future-deprecated). Document the new flag in `.env.local.example`.

## Deliberate deviations from the issue body (approved)

1. `expires_at` (not `expires_at_unix`) — honoring the real backend.
2. New `SiweAuthBridge` instead of editing `WagmiWalletBridge` — keeps the tested
   wallet-sync reducer pure; SIWE stays cleanly flag-gated/removable.
3. No mock auth client (YAGNI) — SIWE runs only against the real backend
   (`SIWE_ENABLED=true, USE_MOCKS=false`); inert in mock mode. Auth is **not**
   added to the 6-method `DarkPoolClient` interface (it's a separate seam).

## Edge cases

- User rejects signature → `error` set, `isAuthenticating=false`, manual retry.
- Nonce/verify network or 4xx/5xx error → `error` set, retryable.
- Token near expiry → `getSessionToken` returns null proactively (request drops to
  no-Bearer; if backend requires auth it 401s → `onUnauthenticated` → prompt).
- Account switch → old session cleared, fresh sign-in for the new address.
- Address comparison normalizes both sides to lowercase (backend returns lowercase;
  wagmi gives checksummed).

## Files

**New:** `session.ts`, `siwe-api.ts`, `siwe-message.ts`, `siwe-action.ts`,
`use-siwe-auth.ts`, `SiweAuthBridge.tsx` (each pure module/hook gets a co-located
`*.test.ts`).
**Edit:** `sdk/client.ts`, `sdk/provider.tsx`, `api-client.ts` (re-export
`useSiweAuth` + `getSessionToken`), `wallet/hooks.ts` & `wallet/index.ts`
(barrels), `wallet/WalletProviders.tsx` (mount bridge), `config.ts`,
`.env.local.example`. **No `package.json` change.** client.ts edits stay in the
options/header region — no conflict with #95's `streamAuctions`.

## Implementation order (test-first, red-green-refactor)

1. `siwe-action.ts` (+test) — pure reducer.
2. `session.ts` (+test) — store, expiry, persistence, in-flight dedupe.
3. `siwe-api.ts` (+test) — nonce/verify fetch + `expires_at` parsing.
4. `siwe-message.ts` (+test) — EIP-4361 builder via viem.
5. `config.ts` (+test) — `NEXT_PUBLIC_SIWE_ENABLED` optional/default-false.
6. `sdk/client.ts` (+test additions) — `getToken`/`onUnauthenticated`, Bearer
   header, 401 hook.
7. `use-siwe-auth.ts` (+test) — orchestration hook (vi.mock wagmi).
8. `SiweAuthBridge.tsx` — wiring; `provider.tsx`, `api-client.ts`, barrels,
   `WalletProviders.tsx` integration.
9. Full `npm run test`, `tsc --noEmit`, `npm run lint` green before PR.

## Verification

- `npm run test` (vitest 4) all green, new units covered.
- `tsc --noEmit` clean (no `any` leaks in the auth seam).
- `npm run lint` clean.
- Manual: with `SIWE_ENABLED=true, USE_MOCKS=false` against a local stack, connect
  → auto-sign → authed call carries Bearer; disconnect clears; expiry re-prompts.
