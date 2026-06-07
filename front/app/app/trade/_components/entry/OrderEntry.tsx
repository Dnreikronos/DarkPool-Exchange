'use client'

// Order entry composition root.
//
// Submission gate: when the placeOrder RPC is mocked (config.useMocks or
// NEXT_PUBLIC_USE_MOCKS_PLACE_ORDER) — or a `placeOrder` prop is injected by
// Storybook/tests — the staged MOCK pipeline runs (fixed delays, mock-store
// push). Otherwise the REAL pipeline runs: build witness → WASM prove →
// ECIES encrypt → POST /v1/orders (#99). Failures surface inline below the
// button via <SubmitError>; success keeps the toast.

import * as React from 'react'

import { config } from '@/lib/config'
import { methodOverridesFromEnv } from '@/lib/api-client'
import { mockStore } from '@/lib/mock-store'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'
import { Decimal } from '@/lib/units'
import { useInternalBalances, useWallet } from '@/lib/wallet/hooks'

import { useToast } from '@/components/ui/use-toast'

import { BuySellTabs } from './BuySellTabs'
import { errorMessage } from '../../_lib/entry/errors'
import { DecimalInput } from './inputs'
import { BASE_TOKEN, FEE_BPS, QUOTE_TOKEN } from '../../_lib/entry/policy'
import { PlaceButton, SubmitError } from './ProveSubmitStages'
import { TotalRow } from './TotalRow'
import {
  buildMockSteps,
  useSubmitStages,
  type SubmitPayload,
} from '../../_hooks/entry/useSubmitStages'
import { useRealSubmission } from '../../_hooks/entry/useRealSubmission'
import { useOrderForm } from '../../_hooks/entry/useOrderForm'
import type { OrderSide } from '../../_lib/entry/validate'

export interface OrderEntryHandle {
  fill: (price: string, side?: OrderSide) => void
}

export interface OrderEntryProps {
  /** Inject the mock-store mutation (Storybook/tests). Forces the mock path. */
  placeOrder?: (payload: SubmitPayload) => void
  /** Injectable wait for the staged mock submission. */
  delay?: (ms: number) => Promise<void>
}

const FEE_FACTOR = new Decimal(1).plus(new Decimal(FEE_BPS).div(10_000))

/** True when the real pipeline should run for placeOrder. */
function realPlaceOrderEnabled(): boolean {
  const override = methodOverridesFromEnv().placeOrder
  const mocked = override ?? config.useMocks
  return !mocked
}

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

  // Real deps are read unconditionally (hooks rules); inert in mock mode.
  // `realBuildSteps` is already memoized inside the hook (deps: trader/prove/client).
  const { buildSteps: realBuildSteps, provingPct: realProvingPct } = useRealSubmission()

  // Mock path: injected placeOrder, else the singleton mock store.
  const effectiveMockPlaceOrder = React.useCallback(
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

  const useReal = !placeOrder && realPlaceOrderEnabled()

  const buildSteps = React.useCallback(
    (payload: SubmitPayload) =>
      useReal
        ? realBuildSteps(payload)
        : buildMockSteps(payload, { placeOrder: effectiveMockPlaceOrder, delay }),
    [useReal, realBuildSteps, effectiveMockPlaceOrder, delay]
  )

  const submit = useSubmitStages({
    buildSteps,
    delay,
    onSuccess: () => {
      toast({
        title: 'Order placed',
        description: 'Pending next auction.',
        variant: 'accent',
      })
      form.reset()
    },
    // Errors render inline via <SubmitError> (no toast) per #99 design.
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
          provingPct={useReal ? realProvingPct : undefined}
          onClick={() => formRef.current?.requestSubmit()}
        />

        <SubmitError phase={submit.phase} />
      </form>
    </section>
  )
})
