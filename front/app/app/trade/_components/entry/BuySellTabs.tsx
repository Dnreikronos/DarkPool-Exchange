'use client'

// Segmented control that picks the order side. Both options are styled
// as `button-ghost` per DESIGN-INSPIRATIONS.md; the active option lifts
// its text to `primary` and its border to `outline-variant`. No semantic
// hue — BUY/SELL are differentiated by position and weight only.

import * as React from 'react'

import { cn } from '@/components/ui/cn'

import type { OrderSide } from './validate'

export interface BuySellTabsProps {
  value: OrderSide
  onChange: (next: OrderSide) => void
  disabled?: boolean
}

interface OptionProps {
  side: OrderSide
  label: string
  active: boolean
  disabled?: boolean
  onClick: () => void
}

function Option({ side, label, active, disabled, onClick }: OptionProps) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      aria-controls="order-entry-form"
      data-side={side}
      data-state={active ? 'active' : 'inactive'}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'flex h-10 flex-1 items-center justify-center',
        'font-mono uppercase tracking-[0.15em] text-[11px] font-medium leading-none',
        'transition-colors duration-150 ease-out',
        'border border-brand-border',
        'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
        active
          ? 'text-brand-fg border-brand-border2 bg-brand-surface'
          : 'text-brand-muted hover:text-brand-fg hover:border-brand-muted',
        'disabled:cursor-not-allowed disabled:opacity-60'
      )}
    >
      {label}
    </button>
  )
}

export function BuySellTabs({ value, onChange, disabled }: BuySellTabsProps) {
  return (
    <div role="tablist" aria-label="Order side" className="flex gap-px">
      <Option
        side="buy"
        label="[ BUY ]"
        active={value === 'buy'}
        disabled={disabled}
        onClick={() => onChange('buy')}
      />
      <Option
        side="sell"
        label="[ SELL ]"
        active={value === 'sell'}
        disabled={disabled}
        onClick={() => onChange('sell')}
      />
    </div>
  )
}
