// Pure formatters for the auction tape. No React, no DOM.
// Timestamps from AuctionSummary.timestampUnix arrive as seconds (bigint).

const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = 60 * 60
const SECONDS_PER_DAY = 60 * 60 * 24

export function formatRelativeTime(auctionUnixSeconds: bigint, nowUnixSeconds: number): string {
  const ageSeconds = Math.max(0, nowUnixSeconds - Number(auctionUnixSeconds))
  if (ageSeconds < SECONDS_PER_MINUTE) return `${ageSeconds}s`
  if (ageSeconds < SECONDS_PER_HOUR) return `${Math.floor(ageSeconds / SECONDS_PER_MINUTE)}m`
  if (ageSeconds < SECONDS_PER_DAY) return `${Math.floor(ageSeconds / SECONDS_PER_HOUR)}h`
  return `${Math.floor(ageSeconds / SECONDS_PER_DAY)}d`
}

const MONTHS = [
  'JAN',
  'FEB',
  'MAR',
  'APR',
  'MAY',
  'JUN',
  'JUL',
  'AUG',
  'SEP',
  'OCT',
  'NOV',
  'DEC',
] as const

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`
}

export function formatFullTimestamp(unixSeconds: bigint): string {
  const d = new Date(Number(unixSeconds) * 1000)
  const hh = pad2(d.getUTCHours())
  const mm = pad2(d.getUTCMinutes())
  const ss = pad2(d.getUTCSeconds())
  const day = pad2(d.getUTCDate())
  const mon = MONTHS[d.getUTCMonth()]
  const year = d.getUTCFullYear()
  return `${hh}:${mm}:${ss} · ${day} ${mon} ${year}`
}

export function formatCount(n: number): string {
  return `${Math.max(0, Math.floor(n))}`
}

/**
 * Seconds remaining until the next auction tick. `intervalSeconds` is the
 * cadence configured on the mock store (default 5). When no auction has
 * landed yet (`lastAuctionUnix === null`), returns a full interval so the
 * countdown shows a valid number on first render.
 *
 * If `now` has drifted past `lastAuctionUnix + intervalSeconds` (e.g. the
 * mock store was paused), wraps with `mod` so the countdown stays inside
 * `[1, intervalSeconds]`.
 */
export function secondsToNextAuction(
  lastAuctionUnixSeconds: bigint | null,
  nowUnixSeconds: number,
  intervalSeconds: number
): number {
  const safeInterval = intervalSeconds > 0 ? Math.floor(intervalSeconds) : 1
  if (lastAuctionUnixSeconds === null) return safeInterval
  const elapsed = nowUnixSeconds - Number(lastAuctionUnixSeconds)
  if (elapsed < 0) return safeInterval
  const remainder = elapsed % safeInterval
  return remainder === 0 ? safeInterval : safeInterval - remainder
}
