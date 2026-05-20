// Display formatters specific to the portfolio panel. Kept separate from
// the tape's formatters so each panel can move independently — the only
// shared dependency is the per-token display precision in
// front/app/app/trade/_components/balances/format-balance.ts (lib-level constant).

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

/** Format a unix-seconds bigint as `HH:MM:SS · DD MON` for the fill table. */
export function formatFillTimestamp(unixSeconds: bigint): string {
  const d = new Date(Number(unixSeconds) * 1000)
  const hh = pad2(d.getUTCHours())
  const mm = pad2(d.getUTCMinutes())
  const ss = pad2(d.getUTCSeconds())
  const day = pad2(d.getUTCDate())
  const mon = MONTHS[d.getUTCMonth()]
  return `${hh}:${mm}:${ss} · ${day} ${mon}`
}

/**
 * Auction-id placeholder rendered in the BATCH column. The wire field is
 * a 14-char hex-ish slug from the mock factory; we truncate to keep the
 * column readable and link-shaped, mirroring the Etherscan-style hashes
 * Phase 2 (#100) will swap in.
 */
export function formatBatch(auctionId: string): string {
  if (auctionId.length <= 12) return auctionId
  return `${auctionId.slice(0, 6)}…${auctionId.slice(-4)}`
}
