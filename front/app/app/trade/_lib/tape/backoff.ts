// Full-jitter exponential backoff (AWS "Exponential Backoff and Jitter").
// Returns a delay in [0, ceiling) where ceiling = min(cap, base * 2**attempt).
// Full jitter (random across the whole window) decorrelates reconnect storms
// when many clients drop at once — better than fixed or equal jitter.

export const BACKOFF_BASE_MS = 1000
export const BACKOFF_CAP_MS = 30_000

export interface BackoffOptions {
  baseMs?: number
  capMs?: number
  /** Injectable for deterministic tests. Defaults to Math.random. */
  random?: () => number
}

export function backoffDelay(attempt: number, opts: BackoffOptions = {}): number {
  const base = opts.baseMs ?? BACKOFF_BASE_MS
  const cap = opts.capMs ?? BACKOFF_CAP_MS
  const random = opts.random ?? Math.random
  const safeAttempt = Math.max(0, attempt)
  const ceiling = Math.min(cap, base * 2 ** safeAttempt)
  return Math.floor(random() * ceiling)
}
