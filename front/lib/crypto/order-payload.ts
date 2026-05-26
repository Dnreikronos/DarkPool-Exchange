export interface DecryptedOrderPayload {
  trader: string
  pair: string
  side: 0 | 1
  price: string
  size: string
  commitment_key: string
  ttl: number
}

export function serializeOrder(payload: DecryptedOrderPayload): Uint8Array {
  if (!payload.trader.startsWith('0x') || payload.trader.length !== 42) {
    throw new Error(`trader must be a 0x-prefixed 20-byte address, got "${payload.trader}"`)
  }
  if (payload.side !== 0 && payload.side !== 1) {
    throw new Error(`side must be 0 (Buy) or 1 (Sell), got ${payload.side}`)
  }
  if (typeof payload.price !== 'string' || payload.price.length === 0) {
    throw new Error('price must be a non-empty string')
  }
  if (typeof payload.size !== 'string' || payload.size.length === 0) {
    throw new Error('size must be a non-empty string')
  }
  if (typeof payload.commitment_key !== 'string' || payload.commitment_key.length === 0) {
    throw new Error('commitment_key must be a non-empty string')
  }

  const json = JSON.stringify({
    trader: payload.trader,
    pair: payload.pair,
    side: payload.side,
    price: payload.price,
    size: payload.size,
    commitment_key: payload.commitment_key,
    ttl: payload.ttl,
  })

  return new TextEncoder().encode(json)
}
