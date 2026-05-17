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
