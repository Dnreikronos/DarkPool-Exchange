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
        className="flex h-10 cursor-pointer select-none list-none items-center gap-2 px-3 font-mono text-label-lg uppercase text-brand-fg transition-colors duration-150 hover:bg-brand-surface focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent [&::-webkit-details-marker]:hidden"
      >
        <span>ETH / USDC</span>
        <Chevron
          aria-hidden="true"
          className={`text-brand-muted transition-transform duration-150 ${open ? 'rotate-180' : ''}`}
        />
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
          ETH / USDC
        </div>
        <div className="mt-2 font-mono text-label-md uppercase text-brand-muted/70">
          MULTI-PAIR · POST-MVP
        </div>
      </div>
    </details>
  )
}

function Chevron(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      width="10"
      height="10"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      strokeLinecap="square"
      {...props}
    >
      <path d="M3 6 L8 11 L13 6" />
    </svg>
  )
}
