import type { Address } from './types'

const HEX_ADDRESS = /^0x[0-9a-fA-F]{40}$/

export function normalizeTraderId(address: Address): string {
  if (!HEX_ADDRESS.test(address)) {
    throw new Error(`Invalid address: ${address}`)
  }
  return address.slice(2).toLowerCase()
}
