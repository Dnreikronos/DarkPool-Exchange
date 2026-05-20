import * as React from 'react'

import { walletStore } from '@/lib/wallet/mock-store'
import { Toaster } from '@/components/ui/toaster'

import { OrderEntry, type OrderEntryHandle } from './OrderEntry'
import { PlaceButton } from './ProveSubmitStages'
import { BuySellTabs } from './BuySellTabs'
import { DecimalInput } from './inputs'
import { TotalRow } from './TotalRow'
import {
  STAGE_DURATIONS_MS,
  STAGE_LABELS,
  STAGE_ORDER,
  STAGE_TOTAL_MS,
  type SubmitStageId,
} from '../../_lib/entry/policy'
import type { SubmissionPhase } from '../../_hooks/entry/useSubmitStages'

// Ladle is the project's visual-verification surface (the JSX test
// transform is node-only). Each story imperatively positions the wallet
// mock store before render; Ladle renders stories in their own iframe so
// per-story side effects don't leak.
//
// Note: the wallet mock (#70) has no balance-mutation API — F1.5 (#72)
// adds deposit/withdraw. Until then `connect()` lands the trader on
// zero internal balances, which the form correctly flags as
// `Insufficient balance.` The `InstantSubmit` story uses a stub
// placeOrder so the SUCCESS path is reviewable; the live mock-store
// path runs once balances are populated by F1.5 in a follow-up.

function useWalletInitialState(initial: 'connected' | 'disconnected') {
  React.useEffect(() => {
    walletStore.disconnect()
    if (initial === 'connected') walletStore.connect()
    return () => walletStore.disconnect()
  }, [initial])
}

function Frame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex w-[360px] flex-col gap-4">
      {children}
      <Toaster />
    </div>
  )
}

// ─── OrderEntry: states ───────────────────────────────────────────────

export const Disconnected = () => {
  useWalletInitialState('disconnected')
  return (
    <Frame>
      <OrderEntry />
    </Frame>
  )
}

export const Connected = () => {
  useWalletInitialState('connected')
  return (
    <Frame>
      <OrderEntry />
    </Frame>
  )
}

export const ClickToFill = () => {
  useWalletInitialState('connected')
  const ref = React.useRef<OrderEntryHandle>(null)
  return (
    <Frame>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => ref.current?.fill('3000.50', 'buy')}
          className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted hover:text-brand-fg border border-brand-border px-3 py-2"
        >
          [ FILL BID 3000.50 ]
        </button>
        <button
          type="button"
          onClick={() => ref.current?.fill('3010.25', 'sell')}
          className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted hover:text-brand-fg border border-brand-border px-3 py-2"
        >
          [ FILL ASK 3010.25 ]
        </button>
      </div>
      <OrderEntry ref={ref} />
    </Frame>
  )
}

export const InstantSubmit = () => {
  // Demonstrates the successful submission path without the staged
  // delays. `placeOrder` is stubbed so the test isn't gated on the
  // wallet store's internal balance (which is zero in Phase 1; F1.5
  // populates it after deposit/withdraw lands). Type a price and a
  // size, then submit — the staged labels still play (delay is a no-op
  // wait but the React state still cycles).
  useWalletInitialState('connected')
  const placed = React.useRef<unknown[]>([])
  return (
    <Frame>
      <OrderEntry placeOrder={(p) => placed.current.push(p)} delay={async () => {}} />
      <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted">
        [ STUBBED PLACEORDER ]
      </p>
    </Frame>
  )
}

// ─── Primitive variants ───────────────────────────────────────────────

export const SidesTab = () => {
  const [side, setSide] = React.useState<'buy' | 'sell'>('buy')
  return (
    <div className="w-[360px]">
      <BuySellTabs value={side} onChange={setSide} />
      <p className="mt-3 font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted">
        SIDE: {side}
      </p>
    </div>
  )
}

export const PriceAndSizeInputs = () => {
  const [price, setPrice] = React.useState('3000')
  const [size, setSize] = React.useState('0.5')
  return (
    <div className="flex w-[360px] flex-col gap-4">
      <DecimalInput
        id="story-price"
        label="[ PRICE · USDC ]"
        unit="USDC"
        value={price}
        onChange={setPrice}
        placeholder="0.00"
      />
      <DecimalInput
        id="story-size"
        label="[ SIZE · WETH ]"
        unit="WETH"
        value={size}
        onChange={setSize}
        placeholder="0.0000"
        rightSlot={
          <button
            type="button"
            onClick={() => setSize('10')}
            className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted hover:text-brand-fg transition-colors"
          >
            [ MAX ]
          </button>
        }
      />
      <TotalRow price={price} size={size} />
    </div>
  )
}

export const PlaceButtonStages = () => {
  // Static gallery of the place button across every submission state.
  // Useful for visually confirming the label mutations and the progress
  // bar position without driving a live timer.
  const states: Array<{ name: string; phase: SubmissionPhase }> = [
    { name: 'idle', phase: { kind: 'idle' } },
    ...STAGE_ORDER.map((stage) => ({
      name: STAGE_LABELS[stage].toLowerCase(),
      phase: {
        kind: 'running' as const,
        stage: stage as SubmitStageId,
        progress: progressMidpoint(stage),
      },
    })),
    { name: 'success', phase: { kind: 'success' } },
    { name: 'error', phase: { kind: 'error', message: 'rejected by engine' } },
  ]
  return (
    <div className="flex w-[360px] flex-col gap-6">
      {states.map(({ name, phase }) => (
        <div key={name} className="flex flex-col gap-2">
          <span className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted">
            [ {name.toUpperCase()} ]
          </span>
          <PlaceButton
            idleLabel="[ BUY · WETH ]"
            phase={phase}
            accent={phase.kind === 'idle'}
            disabled={phase.kind === 'error'}
            onClick={() => {}}
          />
        </div>
      ))}
    </div>
  )
}

function progressMidpoint(stage: SubmitStageId): number {
  let elapsed = 0
  for (const id of STAGE_ORDER) {
    if (id === stage) {
      elapsed += STAGE_DURATIONS_MS[id] / 2
      break
    }
    elapsed += STAGE_DURATIONS_MS[id]
  }
  return elapsed / STAGE_TOTAL_MS
}
