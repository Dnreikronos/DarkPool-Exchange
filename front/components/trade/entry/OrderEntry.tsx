'use client'

// Order entry composition root.
//
// Phase 1 wiring:
//   - reads internal balances via the mock wallet store (F1.3 / #70)
//   - pushes new orders into the mock store (F1.2 / #69) where they
//     immediately surface in the orderbook (F1.6 / #73) and the
//     my-orders panel (F1.10 / #77).
//   - surfaces the staged mock UX (ENCRYPT → PROVE → SUBMIT → ACK) so
//     the WASM prover delay (5–30s in Phase 2) has a UX rehearsed before
//     the real flow lands at #99.
//
// Click-to-fill: the orderbook (#73) calls `onPriceSelect(price, side)`.
// `OrderEntry` exposes an imperative `fill(price, side?)` via forwardRef
// so a parent composing the trade layout can pipe that callback in:
//
//   const ref = React.useRef<OrderEntryHandle>(null)
//   <OrderBook onPriceSelect={(p, s) => ref.current?.fill(p, ...)} />
//   <OrderEntry ref={ref} />
//
// Shell.tsx (F1.1) is out of this issue's file scope — the wiring lands
// in a follow-up that owns Shell.

import * as React from 'react'

import { mockStore } from '../../../lib/mock-store'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import { Decimal } from '../../../lib/units'
import { useInternalBalances, useWallet } from '../../../lib/wallet/hooks'

import { useToast } from '../../ui/use-toast'

import { BuySellTabs } from './BuySellTabs'
import { errorMessage } from './errors'
import { DecimalInput } from './inputs'
import { BASE_TOKEN, FEE_BPS, QUOTE_TOKEN } from './policy'
import { PlaceButton } from './ProveSubmitStages'
import { TotalRow } from './TotalRow'
import { useOrderForm } from './useOrderForm'
import { useSubmitStages, type SubmitPayload } from './useSubmitStages'
import type { OrderSide } from './validate'

export interface OrderEntryHandle {
  /** Fill the form from an external source (orderbook click-to-fill). */
  fill: (price: string, side?: OrderSide) => void
}

export interface OrderEntryProps {
  /**
   * Injectable side-effect: where to push the placed order. Defaults to
   * the singleton mock store. Tests and Storybook variants override
   * this to stub the mutation.
   */
  placeOrder?: (payload: SubmitPayload) => void
  /**
   * Injectable wait function for the staged submission. Defaults to
   * setTimeout-based sleep. Storybook variants pass `() => Promise.resolve()`
   * to step through stages without the real timings.
   */
  delay?: (ms: number) => Promise<void>
}

const FEE_FACTOR = new Decimal(1).plus(new Decimal(FEE_BPS).div(10_000))

export const OrderEntry = React.forwardRef<OrderEntryHandle, OrderEntryProps>(function OrderEntry(
  { placeOrder, delay },
  ref
) {
  const { isConnected } = useWallet()
  const balances = useInternalBalances()
  const { toast } = useToast()
  const formRef = React.useRef<HTMLFormElement>(null)
  const headerId = React.useId()
  const priceErrorId = React.useId()
  const sizeErrorId = React.useId()
  const formErrorId = React.useId()

  const form = useOrderForm({
    isConnected,
    baseBalance: balances.weth,
    quoteBalance: balances.usdc,
  })
  const formStateRef = React.useRef(form)
  formStateRef.current = form

  const effectivePlaceOrder = React.useCallback(
    (payload: SubmitPayload) => {
      if (placeOrder) {
        placeOrder(payload)
        return
      }
      mockStore.getState().placeOrder({
        side: payload.side === 'buy' ? Side.BUY : Side.SELL,
        price: payload.price,
        size: payload.size,
      })
    },
    [placeOrder]
  )

  const submit = useSubmitStages({
    placeOrder: effectivePlaceOrder,
    delay,
    onSuccess: () => {
      toast({
        title: 'Order placed',
        description: 'Pending next auction.',
        variant: 'accent',
      })
      form.reset()
    },
    onError: (error) => {
      toast({
        title: 'Order rejected',
        description: error.message,
      })
    },
  })

  React.useImperativeHandle(
    ref,
    () => ({
      fill: (price: string, nextSide?: OrderSide) => {
        formStateRef.current.fillFromLevel(price, nextSide)
      },
    }),
    []
  )

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!form.validation.ok || submit.isRunning) return
    void submit.submit({ side: form.side, price: form.price, size: form.size })
  }

  const handleMax = () => {
    // Sell: cap size at the available WETH balance.
    // Buy: derive size = quoteBalance / (price * (1 + fee_bps/10000)),
    // snap down to 4dp so the displayed value matches the size column.
    // If price isn't set we leave the field alone — populating it from
    // a stale or zero price would mislead the user.
    try {
      if (form.side === 'sell') {
        form.setSize(balances.weth)
        return
      }
      if (form.price.trim() === '') return
      const priceD = new Decimal(form.price)
      if (priceD.lte(0)) return
      const quoteD = new Decimal(balances.usdc)
      const maxSize = quoteD.div(FEE_FACTOR).div(priceD)
      form.setSize(maxSize.toDecimalPlaces(4, Decimal.ROUND_DOWN).toFixed())
    } catch {
      /* invalid input — leave the size field as-is */
    }
  }

  const priceError = form.validation.errors.price
  const sizeError = form.validation.errors.size
  const formError = form.validation.errors.form

  const idleLabel = `${form.side === 'buy' ? '[ BUY' : '[ SELL'} · ${BASE_TOKEN} ]`
  const accentActive = form.validation.ok && submit.phase.kind !== 'running'

  return (
    <section
      aria-labelledby={headerId}
      className="flex h-full flex-col border border-brand-border bg-brand-surface"
    >
      <header className="flex h-9 items-center border-b border-brand-border px-4">
        <span
          id={headerId}
          className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
        >
          [ ORDER ENTRY ]
        </span>
      </header>
      <form
        ref={formRef}
        id="order-entry-form"
        onSubmit={handleSubmit}
        aria-describedby={formError ? formErrorId : undefined}
        className="flex flex-1 flex-col gap-4 p-4"
        noValidate
      >
        <BuySellTabs value={form.side} onChange={form.setSide} disabled={submit.isRunning} />

        <DecimalInput
          id="order-entry-price"
          label={`[ PRICE · ${QUOTE_TOKEN} ]`}
          unit={QUOTE_TOKEN}
          value={form.price}
          onChange={form.setPrice}
          placeholder="0.00"
          disabled={submit.isRunning}
          invalid={!!priceError}
          errorId={priceError ? priceErrorId : undefined}
        />
        {priceError && (
          <p id={priceErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(priceError)}
          </p>
        )}

        <DecimalInput
          id="order-entry-size"
          label={`[ SIZE · ${BASE_TOKEN} ]`}
          unit={BASE_TOKEN}
          value={form.size}
          onChange={form.setSize}
          placeholder="0.0000"
          disabled={submit.isRunning}
          invalid={!!sizeError}
          errorId={sizeError ? sizeErrorId : undefined}
          rightSlot={
            <button
              type="button"
              onClick={handleMax}
              disabled={submit.isRunning || !isConnected}
              className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted hover:text-brand-fg transition-colors disabled:cursor-not-allowed disabled:opacity-50"
              aria-label="Set size to maximum"
            >
              [ MAX ]
            </button>
          }
        />
        {sizeError && (
          <p id={sizeErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(sizeError)}
          </p>
        )}

        <TotalRow price={form.price} size={form.size} />

        {formError && (
          <p id={formErrorId} role="alert" className="font-mono text-body-sm text-brand-muted">
            {errorMessage(formError)}
          </p>
        )}

        <PlaceButton
          idleLabel={idleLabel}
          phase={submit.phase}
          disabled={!form.validation.ok}
          accent={accentActive}
          onClick={() => formRef.current?.requestSubmit()}
        />
      </form>
    </section>
  )
})
