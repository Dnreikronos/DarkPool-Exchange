import type { Address } from './types'

/**
 * Pure transition reducer for `WagmiWalletBridge`.
 *
 * Given the previously-observed address and wagmi's current
 * `(status, address)` tuple, returns the action the bridge should
 * apply this tick. Extracted so we can unit-test the account-switch,
 * reconnect, and disconnect cases without standing up a wagmi
 * provider tree.
 */
export type WagmiAccountStatus = 'connected' | 'connecting' | 'reconnecting' | 'disconnected'

export type BridgeAction =
  | { kind: 'connect'; address: Address; clearCaches: boolean }
  | { kind: 'disconnect'; clearCaches: boolean }
  | { kind: 'noop' }

export function computeBridgeAction(
  previousAddress: Address | null,
  status: WagmiAccountStatus,
  nextAddress: Address | null
): BridgeAction {
  if (status === 'connected' && nextAddress) {
    // Account switch (A → B) is the case the connected→disconnected
    // guard alone misses: wagmi stays `connected` through it, so we
    // detect it by comparing addresses against the previous tick.
    const isSwitch = previousAddress !== null && previousAddress !== nextAddress
    return { kind: 'connect', address: nextAddress, clearCaches: isSwitch }
  }
  if (status === 'disconnected') {
    // Only clear on the *transition* into disconnected. Repeated
    // disconnected ticks (e.g. SSR hydration before wagmi attaches)
    // must not nuke the cache.
    const wasConnected = previousAddress !== null
    return { kind: 'disconnect', clearCaches: wasConnected }
  }
  // `connecting` / `reconnecting`: leave the store alone until wagmi
  // resolves to a terminal state.
  return { kind: 'noop' }
}
