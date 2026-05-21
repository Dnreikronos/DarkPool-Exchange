/**
 * Per-trader localStorage entries we clear on disconnect so that the
 * next address starts from a clean slate.
 *
 * We drop the `dp:` namespace — anything the trading app intentionally
 * writes under a darkpool-owned key. We deliberately do NOT touch
 * `wagmi.*`: wagmi manages its own persistence (session token,
 * recent-connector id, …) through its own storage adapter and clears
 * what it needs to on disconnect. Bulk-deleting that namespace would
 * fight with wagmi's state machine.
 */
const PER_TRADER_PREFIXES = ['dp:'] as const

export function clearPerTraderLocalStorage(): void {
  if (typeof window === 'undefined') return
  const storage = window.localStorage
  // Snapshot first — we mutate while iterating otherwise.
  const keys: string[] = []
  for (let i = 0; i < storage.length; i += 1) {
    const key = storage.key(i)
    if (key && PER_TRADER_PREFIXES.some((prefix) => key.startsWith(prefix))) {
      keys.push(key)
    }
  }
  for (const key of keys) {
    storage.removeItem(key)
  }
}
