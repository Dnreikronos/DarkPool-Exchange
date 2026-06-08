'use client'

// Segmented control that picks the order side. Both options are styled
// as `button-ghost` per DESIGN-INSPIRATIONS.md; the active option lifts
// its text to `primary` and its border to `outline-variant`. No semantic
// hue — BUY/SELL are differentiated by position and weight only.

import * as React from 'react'

import { cn } from '@/components/ui/cn'

import type { OrderSide } from '../../_lib/entry/validate'

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

const Option = React.forwardRef<HTMLButtonElement, OptionProps>(function Option(
  { side, label, active, disabled, onClick },
  ref
) {
  return (
    <button
      ref={ref}
      type="button"
      role="tab"
      aria-selected={active}
      aria-controls="order-entry-form"
      data-side={side}
      data-state={active ? 'active' : 'inactive'}
      // Roving tabindex: one Tab stop for the whole tablist; arrows move
      // the selection (see BuySellTabs.onKeyDown).
      tabIndex={active ? 0 : -1}
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
})

export function BuySellTabs({ value, onChange, disabled }: BuySellTabsProps) {
  const buyRef = React.useRef<HTMLButtonElement>(null)
  const sellRef = React.useRef<HTMLButtonElement>(null)

  // ARIA tabs keyboard contract (#80): Left/Right move the selection
  // (selection follows focus — only two tabs), Home/End jump to the
  // first/last tab.
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (disabled) return
    let next: OrderSide
    switch (event.key) {
      case 'ArrowLeft':
      case 'ArrowRight':
        next = value === 'buy' ? 'sell' : 'buy'
        break
      case 'Home':
        next = 'buy'
        break
      case 'End':
        next = 'sell'
        break
      default:
        return
    }
    event.preventDefault()
    if (next !== value) onChange(next)
    ;(next === 'buy' ? buyRef : sellRef).current?.focus()
  }

  return (
    <div role="tablist" aria-label="Order side" className="flex gap-px" onKeyDown={onKeyDown}>
      <Option
        ref={buyRef}
        side="buy"
        label="[ BUY ]"
        active={value === 'buy'}
        disabled={disabled}
        onClick={() => onChange('buy')}
      />
      <Option
        ref={sellRef}
        side="sell"
        label="[ SELL ]"
        active={value === 'sell'}
        disabled={disabled}
        onClick={() => onChange('sell')}
      />
    </div>
  )
}
