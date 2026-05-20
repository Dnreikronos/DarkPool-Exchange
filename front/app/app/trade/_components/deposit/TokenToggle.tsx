'use client'

import * as React from 'react'

import { cn } from '@/components/ui/cn'
import type { TokenSymbol } from '@/lib/wallet/types'

interface TokenToggleProps {
  value: TokenSymbol
  onChange: (token: TokenSymbol) => void
  disabled?: boolean
  /** Aria label for the radiogroup container. */
  label: string
}

const TOKENS: readonly TokenSymbol[] = ['WETH', 'USDC']

export function TokenToggle({ value, onChange, disabled, label }: TokenToggleProps) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="grid grid-cols-2 border border-brand-border"
    >
      {TOKENS.map((token) => {
        const active = token === value
        return (
          <button
            key={token}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            onClick={() => onChange(token)}
            className={cn(
              'h-10 px-3 font-mono text-label-lg uppercase tracking-label',
              'transition-colors duration-150 ease-out',
              'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent',
              'disabled:cursor-not-allowed disabled:text-brand-muted',
              active
                ? 'bg-brand-surface text-brand-fg'
                : 'bg-transparent text-brand-muted hover:text-brand-fg'
            )}
          >
            [ {token} ]
          </button>
        )
      })}
    </div>
  )
}
