'use client'

import * as React from 'react'

import { useInternalBalances, useWallet } from '@/lib/wallet/hooks'

import { DivergenceBanner } from './DivergenceBanner'
import { FillHistoryTable } from './FillHistoryTable'
import { PnLCard } from './PnLCard'
import { usePortfolio } from './usePortfolio'

/**
 * Portfolio surface: P&L summary + fill history. Mounts inside
 * /app/portfolio (see app/app/portfolio/page.tsx).
 *
 * The disconnected state still surfaces the fill table so a returning
 * trader can see what the session captured pre-disconnect, but the
 * stat triplet collapses to a hint about wallet status.
 */
export function PortfolioPanel(): JSX.Element {
  const { isConnected } = useWallet()
  const internal = useInternalBalances()
  const { fills, summary, divergence } = usePortfolio(internal)

  return (
    <div className="flex flex-col gap-4 px-4 py-6 sm:px-6 lg:px-8 lg:py-8">
      <PageHeader />
      {isConnected ? (
        <>
          <PnLCard summary={summary} />
          <DivergenceBanner result={divergence} />
        </>
      ) : (
        <Disconnected />
      )}
      <FillHistoryTable fills={fills} />
    </div>
  )
}

function PageHeader() {
  return (
    <header className="flex flex-col gap-1">
      <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        [ PORTFOLIO · ETH / USDC ]
      </span>
      <h1 className="font-display text-display-md tracking-brand text-brand-fg">POSITIONS</h1>
    </header>
  )
}

function Disconnected() {
  return (
    <section
      aria-label="Wallet disconnected"
      className="border border-brand-border bg-brand-surface px-5 py-6"
    >
      <p
        role="status"
        className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted"
      >
        [ CONNECT WALLET TO SEE POSITION + P&L ]
      </p>
    </section>
  )
}
