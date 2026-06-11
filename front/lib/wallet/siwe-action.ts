import type { Address } from './types'
import type { WagmiAccountStatus } from './bridge-action'

/**
 * Pure transition reducer for `SiweAuthBridge`.
 *
 * Decides whether connecting/switching/disconnecting the wallet should
 * trigger a SIWE sign-in, clear the session, or do nothing — without
 * standing up a wagmi provider tree. Mirrors `computeBridgeAction`.
 *
 * The key invariant: auto sign-in fires only on an address *transition*
 * (a fresh connect or an account switch). When the same address is still
 * connected but lost its session (token expired or a 401 cleared it), we
 * return `noop` — the user re-signs manually via a prompt, never a
 * surprise wallet popup.
 *
 * `sessionAddress` is the address of the current *valid* session (or
 * `null` when there is none / it expired). A page refresh that rehydrates
 * a still-valid token for the reconnecting address is therefore a noop —
 * no re-signing required.
 */
export type SiweAction =
  | { kind: 'sign-in'; address: Address }
  | { kind: 'sign-out' }
  | { kind: 'noop' }

function sameAddress(a: Address | null, b: Address | null): boolean {
  return a !== null && b !== null && a.toLowerCase() === b.toLowerCase()
}

export function computeSiweAction(
  previousAddress: Address | null,
  status: WagmiAccountStatus,
  nextAddress: Address | null,
  sessionAddress: Address | null
): SiweAction {
  if (status === 'connected' && nextAddress) {
    // Already authenticated for this address (e.g. token rehydrated on refresh).
    if (sameAddress(sessionAddress, nextAddress)) return { kind: 'noop' }
    // First time we observe this address with no matching session → sign in.
    if (previousAddress === null) return { kind: 'sign-in', address: nextAddress }
    // Account switch A → B → sign in for the new address.
    if (!sameAddress(previousAddress, nextAddress)) {
      return { kind: 'sign-in', address: nextAddress }
    }
    // Same address, session gone (expiry / 401): re-sign is manual.
    return { kind: 'noop' }
  }
  if (status === 'disconnected') {
    // Only act on the transition *into* disconnected.
    return previousAddress !== null ? { kind: 'sign-out' } : { kind: 'noop' }
  }
  // `connecting` / `reconnecting`: wait for a terminal state.
  return { kind: 'noop' }
}
