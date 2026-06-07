import type { Address } from './types'

/**
 * SIWE session store — the single source of truth for the per-user
 * session JWT, framework-agnostic so the SDK's `RestClient` can read the
 * live token per request via {@link getSessionToken} without going
 * through React.
 *
 * Persisted to `localStorage` under the `dp:` prefix so the existing
 * `clearPerTraderLocalStorage()` wipes it on account switch / disconnect.
 * Expiry is enforced proactively: an expired session reads as no session.
 */
export const SESSION_STORAGE_KEY = 'dp:session-token'

export interface Session {
  /** Backend-issued HS256 JWT, sent as `Authorization: Bearer <token>`. */
  token: string
  /** Unix seconds (backend `expires_at`); the session is invalid once `now >= expiresAt`. */
  expiresAt: number
  /** The wallet address this session authenticates. */
  address: Address
}

let current: Session | null = null
const listeners = new Set<() => void>()

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000)
}

function storage(): Storage | null {
  return typeof window === 'undefined' ? null : window.localStorage
}

function notify(): void {
  for (const listener of listeners) listener()
}

function isValid(session: Session | null): session is Session {
  return session !== null && session.expiresAt > nowSeconds()
}

function parseSession(raw: string): Session | null {
  try {
    const value = JSON.parse(raw) as Partial<Session>
    if (
      typeof value.token === 'string' &&
      typeof value.expiresAt === 'number' &&
      typeof value.address === 'string'
    ) {
      return { token: value.token, expiresAt: value.expiresAt, address: value.address as Address }
    }
    return null
  } catch {
    return null
  }
}

/** Rehydrate the in-memory session from storage, dropping it if expired or malformed. */
export function loadSession(): void {
  const store = storage()
  const raw = store?.getItem(SESSION_STORAGE_KEY) ?? null
  const parsed = raw !== null ? parseSession(raw) : null
  if (isValid(parsed)) {
    current = parsed
  } else {
    current = null
    store?.removeItem(SESSION_STORAGE_KEY)
  }
  notify()
}

/** The current valid session, or `null` if absent/expired. */
export function getSession(): Session | null {
  return isValid(current) ? current : null
}

/** The current session token if valid, else `null`. Read per-request by the SDK. */
export function getSessionToken(): string | null {
  const session = getSession()
  return session ? session.token : null
}

export function setSession(session: Session): void {
  current = session
  storage()?.setItem(SESSION_STORAGE_KEY, JSON.stringify(session))
  notify()
}

export function clearSession(): void {
  current = null
  storage()?.removeItem(SESSION_STORAGE_KEY)
  notify()
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** Raw snapshot for `useSyncExternalStore`; stable until the session changes. */
export function getSnapshot(): Session | null {
  return current
}

/** SSR snapshot — no session on the server. */
export function getServerSnapshot(): Session | null {
  return null
}
