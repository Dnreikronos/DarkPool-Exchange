'use client'

// Form state for the order-entry panel. Holds the controlled side /
// price / size strings and exposes a `setPrice(price, side?)` setter
// the orderbook's click-to-fill flow can call from outside.
//
// Validation is derived (not stored) so it stays in sync with whatever
// the latest balances are without an extra effect. Balance changes
// triggered by the mock store (auctions clearing, deposits) re-render
// the panel and the derived `errors` follow.

import { useCallback, useMemo, useState } from 'react'

import { validateOrder, type OrderSide, type ValidationResult } from '../../_lib/entry/validate'

export interface OrderFormState {
  side: OrderSide
  price: string
  size: string
}

export interface UseOrderFormParams {
  /** Internal WETH balance (decimal string). */
  baseBalance: string
  /** Internal USDC balance (decimal string). */
  quoteBalance: string
  isConnected: boolean
  initial?: Partial<OrderFormState>
}

export interface UseOrderFormResult extends OrderFormState {
  setSide: (next: OrderSide) => void
  setPrice: (next: string) => void
  setSize: (next: string) => void
  /** Set price and (optionally) side in one go. Used by click-to-fill. */
  fillFromLevel: (price: string, side?: OrderSide) => void
  reset: () => void
  validation: ValidationResult
}

export function useOrderForm(params: UseOrderFormParams): UseOrderFormResult {
  const [side, setSide] = useState<OrderSide>(params.initial?.side ?? 'buy')
  const [price, setPrice] = useState(params.initial?.price ?? '')
  const [size, setSize] = useState(params.initial?.size ?? '')

  const fillFromLevel = useCallback((nextPrice: string, nextSide?: OrderSide) => {
    setPrice(nextPrice)
    if (nextSide) setSide(nextSide)
  }, [])

  const reset = useCallback(() => {
    setSize('')
    // Keep `price` and `side` — the user usually places a series of orders
    // around the same level, and clearing them every time wastes input.
  }, [])

  const validation = useMemo(
    () =>
      validateOrder({
        side,
        price,
        size,
        baseBalance: params.baseBalance,
        quoteBalance: params.quoteBalance,
        isConnected: params.isConnected,
      }),
    [side, price, size, params.baseBalance, params.quoteBalance, params.isConnected]
  )

  return {
    side,
    price,
    size,
    setSide,
    setPrice,
    setSize,
    fillFromLevel,
    reset,
    validation,
  }
}
