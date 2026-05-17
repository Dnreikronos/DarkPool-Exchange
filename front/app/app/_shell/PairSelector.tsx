'use client'

import { useState } from 'react'

export function PairSelector() {
  const [open, setOpen] = useState(false)
  return (
    <details
      className="relative"
      onToggle={(event) =>
        setOpen((event.currentTarget as HTMLDetailsElement).open)
      }
    >
      <summary
        aria-label="Trading pair selector"
        className="flex h-10 cursor-pointer select-none list-none items-center border border-brand-border2 px-4 font-mono text-label-lg uppercase text-brand-fg transition-colors duration-150 hover:border-brand-muted focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent [&::-webkit-details-marker]:hidden"
      >
        <span>[ ETH / USDC ]</span>
        <span
          aria-hidden="true"
          className={`ml-3 font-mono text-label-md text-brand-muted transition-transform duration-150 ${
            open ? 'rotate-180' : ''
          }`}
        >
          ▾
        </span>
      </summary>
      <div
        role="listbox"
        aria-label="Trading pairs"
        className="absolute right-0 top-full z-30 mt-1 min-w-full whitespace-nowrap border border-brand-border bg-brand-surface p-3"
      >
        <div
          role="option"
          aria-selected="true"
          className="font-mono text-label-lg uppercase text-brand-fg"
        >
          [ ETH / USDC ]
        </div>
        <div className="mt-2 font-mono text-label-md uppercase text-brand-muted/70">
          MULTI-PAIR · POST-MVP
        </div>
      </div>
    </details>
  )
}
