import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import type { MyOrderStatus } from './types'

/**
 * Render a UNIX-second timestamp as `HH:MM:SS` in UTC. The mock-store
 * stamps `submittedAtUnix` from the injectable clock, so a UTC render
 * keeps tests reproducible regardless of the runner's locale.
 */
export function formatSubmittedAt(unixSec: bigint): string {
  const ms = Number(unixSec) * 1000
  const d = new Date(ms)
  const hh = String(d.getUTCHours()).padStart(2, '0')
  const mm = String(d.getUTCMinutes()).padStart(2, '0')
  const ss = String(d.getUTCSeconds()).padStart(2, '0')
  return `${hh}:${mm}:${ss}`
}

export function sideLabel(side: Side): string {
  return side === Side.BUY ? '[ BUY ]' : '[ SELL ]'
}

const STATUS_LABEL: Record<MyOrderStatus, string> = {
  open: '[ OPEN ]',
  filled: '[ FILLED ]',
  cancelled: '[ CANCELLED ]',
}

export function statusLabel(status: MyOrderStatus): string {
  return STATUS_LABEL[status]
}
