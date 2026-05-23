/**
 * Persistence for the first-run onboarding modal.
 *
 * Acceptance criterion: "Dismiss persists per wallet." We key the
 * dismissal flag by lowercased wallet address; when no wallet is yet
 * connected we still need a usable bucket so the modal does not
 * re-pop on every reload of the disconnected app, so we fall back to
 * an `anon` bucket. The on-connect transition can promote the anon
 * dismissal to the address-keyed entry — see `promoteAnonToAddress`.
 *
 * The module is intentionally storage-adapter shaped (it takes a
 * `Storage`-like object, not `window.localStorage` directly) so the
 * pure helpers stay unit-testable in node without a JSDOM environment.
 * The runtime call sites pass `window.localStorage`; tests pass an
 * in-memory `Map`-backed adapter (see storage.test.ts).
 */

const PREFIX = 'dp:onboarding-dismissed:'
const ANON_KEY = 'anon'
const FLAG = '1'

export interface StorageLike {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

function normalize(address: string | null | undefined): string {
  if (!address) return ANON_KEY
  return address.toLowerCase()
}

function keyFor(address: string | null | undefined): string {
  return PREFIX + normalize(address)
}

export function isDismissed(storage: StorageLike, address: string | null | undefined): boolean {
  return storage.getItem(keyFor(address)) === FLAG
}

export function setDismissed(storage: StorageLike, address: string | null | undefined): void {
  storage.setItem(keyFor(address), FLAG)
  // Mirror to the anon bucket so a subsequent disconnect doesn't pop the
  // modal again. This is the symmetric counterpart to
  // `promoteAnonToAddress`: the user's intent ("I've seen this") is
  // wallet-agnostic, but the storage keys are wallet-specific, so we
  // keep both buckets in sync at write time.
  if (normalize(address) !== ANON_KEY) {
    storage.setItem(PREFIX + ANON_KEY, FLAG)
  }
}

export function clearDismissed(storage: StorageLike, address: string | null | undefined): void {
  storage.removeItem(keyFor(address))
}

/**
 * When the wallet connects, copy any prior anon-bucket dismissal forward
 * so the user doesn't see the modal again as soon as they connect.
 * No-op when there is no prior anon dismissal or the address is empty.
 */
export function promoteAnonToAddress(storage: StorageLike, address: string): void {
  if (!address) return
  if (storage.getItem(PREFIX + ANON_KEY) !== FLAG) return
  storage.setItem(keyFor(address), FLAG)
}
