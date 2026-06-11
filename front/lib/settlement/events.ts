// Decoded-log → SettlementEvent mapping for the BatchSettled watcher.
// Kept separate from the hook so the conversion is unit-testable without
// wagmi in the loop.

import type { SettlementEvent } from './correlate'

/**
 * The slice of a viem `Log` (as delivered by `useWatchContractEvent`
 * with `eventName: 'BatchSettled'`) that the mapping needs. `args` is
 * partial because viem leaves fields undefined when decoding fails, and
 * `transactionHash` is null for logs from pending blocks.
 */
export interface BatchSettledLog {
  args: { batchId?: string; timestamp?: bigint }
  transactionHash: string | null
}

/** Maps decoded logs to settlement events, dropping undecodable ones. */
export function settlementEventsFromLogs(logs: readonly BatchSettledLog[]): SettlementEvent[] {
  const events: SettlementEvent[] = []
  for (const log of logs) {
    const { batchId, timestamp } = log.args
    if (!batchId || timestamp === undefined || !log.transactionHash) continue
    events.push({ batchId, txHash: log.transactionHash, timestampUnix: timestamp })
  }
  return events
}
