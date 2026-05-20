'use client'

// Labeled decimal inputs for price and size. The label sits above the
// field (uppercase tracked) per the `input-text` + `input-label` pattern
// in DESIGN.md. The suffix tag (`USDC`, `WETH`) sits inside the field on
// the right, rendered in `label-md` muted so it reads as metadata.
//
// Values are strings the whole way through — never coerce to JS number.
// `inputMode="decimal"` opens the right keypad on mobile without forcing
// a numeric input (which would round / coerce).

import * as React from 'react'

import { cn } from '../../ui/cn'

const DECIMAL_PATTERN = /^\d*\.?\d*$/

export interface DecimalInputProps {
  id: string
  label: string
  /** Suffix tag rendered inside the field (e.g. "USDC"). */
  unit?: string
  value: string
  onChange: (next: string) => void
  placeholder?: string
  disabled?: boolean
  errorId?: string
  invalid?: boolean
  /**
   * Optional control rendered inside the right end of the field. Used
   * for the SizeInput MAX shortcut.
   */
  rightSlot?: React.ReactNode
}

export const DecimalInput = React.forwardRef<HTMLInputElement, DecimalInputProps>(
  function DecimalInput(
    { id, label, unit, value, onChange, placeholder, disabled, errorId, invalid, rightSlot },
    ref
  ) {
    const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
      const next = event.target.value
      // Reject anything that isn't a partial decimal so we don't push
      // mangled strings down to Decimal.js on every keystroke.
      if (next === '' || DECIMAL_PATTERN.test(next)) onChange(next)
    }

    return (
      <div className="flex flex-col gap-1">
        <label
          htmlFor={id}
          className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
        >
          {label}
        </label>
        <div
          className={cn(
            'relative flex h-10 items-stretch border bg-brand-surface',
            invalid ? 'border-brand-border2' : 'border-brand-border',
            'focus-within:border-brand-accent',
            disabled && 'opacity-60'
          )}
        >
          <input
            ref={ref}
            id={id}
            type="text"
            inputMode="decimal"
            autoComplete="off"
            spellCheck={false}
            value={value}
            onChange={handleChange}
            disabled={disabled}
            placeholder={placeholder}
            aria-invalid={invalid || undefined}
            aria-describedby={errorId}
            className={cn(
              'flex-1 bg-transparent px-3 font-mono tabular-nums text-[12px] leading-[1.8] text-brand-fg',
              'placeholder:text-brand-muted',
              'focus:outline-none',
              'disabled:cursor-not-allowed'
            )}
          />
          {(rightSlot || unit) && (
            <div className="flex shrink-0 items-center gap-2 pr-3">
              {rightSlot}
              {unit && (
                <span
                  aria-hidden
                  className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
                >
                  {unit}
                </span>
              )}
            </div>
          )}
        </div>
      </div>
    )
  }
)
