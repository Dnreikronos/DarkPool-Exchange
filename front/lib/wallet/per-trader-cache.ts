/**
 * Per-trader localStorage entries we clear on disconnect so that the
 * next address starts from a clean slate.
 *
 * Two namespaces are dropped:
 *   - the `dp:` prefix: anything the trading app intentionally writes
 *     under a darkpool-owned key,
 *   - `wagmi.*`: wagmi's own storage. Wagmi already clears its own
 *     session token on disconnect, but cached `recentConnectorId` etc
 *     can keep referencing a stale account; nuking the namespace is
 *     the safest call when the user explicitly disconnects.
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
