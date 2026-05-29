'use client'

import * as React from 'react'

import { NumericText } from '@/components/NumericText'
import { useInternalBalances, useWallet, useWalletBalances } from '@/lib/wallet/hooks'
import type { Balances, TokenSymbol } from '@/lib/wallet/types'
import { displayDecimalsFor } from '../../_lib/balances/format-balance'
import { BalancesDisconnected } from './states'

const TOKEN_ROWS: readonly TokenSymbol[] = ['WETH', 'USDC']

const COLUMN_TAGS = ['[ WALLET ]', '[ DARKPOOL ]'] as const

export function BalancesPanel() {
  const { isConnected } = useWallet()
  const wallet = useWalletBalances()
  const internal = useInternalBalances()
  const headerId = React.useId()

  return (
    <section
      aria-labelledby={headerId}
      className="flex h-full flex-col border border-brand-border bg-brand-surface"
    >
      <Header id={headerId} />
      {isConnected ? (
        <BalancesGrid wallet={wallet} internal={internal} />
      ) : (
        <BalancesDisconnected />
      )}
    </section>
  )
}

function Header({ id }: { id: string }) {
  return (
    <header className="flex h-9 items-center border-b border-brand-border px-4">
      <span id={id} className="font-mono text-label-md uppercase text-brand-muted">
        [ BALANCES ]
      </span>
    </header>
  )
}

interface BalancesGridProps {
  wallet: Balances
  internal: Balances
}

function BalancesGrid({ wallet, internal }: BalancesGridProps) {
  return (
    <div className="flex flex-1 flex-col">
      <ColumnHeaderRow />
      {TOKEN_ROWS.map((symbol) => (
        <TokenRow
          key={symbol}
          symbol={symbol}
          walletAmount={amountFor(wallet, symbol)}
          internalAmount={amountFor(internal, symbol)}
        />
      ))}
    </div>
  )
}

function ColumnHeaderRow() {
  return (
    <div className="grid grid-cols-[44px_1fr_1fr] items-center gap-x-3 border-b border-brand-border px-3 py-2">
      <span aria-hidden className="font-mono text-label-md uppercase text-brand-muted" />
      {COLUMN_TAGS.map((tag) => (
        <span
          key={tag}
          className="whitespace-nowrap text-right font-mono text-label-md uppercase text-brand-muted"
        >
          {tag}
        </span>
      ))}
    </div>
  )
}

interface TokenRowProps {
  symbol: TokenSymbol
  walletAmount: string
  internalAmount: string
}

function TokenRow({ symbol, walletAmount, internalAmount }: TokenRowProps) {
  const dp = displayDecimalsFor(symbol)
  return (
    <div className="grid grid-cols-[44px_1fr_1fr] items-baseline gap-x-3 border-b border-brand-border px-3 py-3 last:border-b-0">
      <span className="font-mono text-label-lg uppercase tracking-label text-brand-fg">
        {symbol}
      </span>
      <NumericText
        value={walletAmount}
        decimals={dp}
        kind="size"
        aria-label={`${symbol} wallet balance`}
        className="text-body-md text-brand-fg"
      />
      <NumericText
        value={internalAmount}
        decimals={dp}
        kind="size"
        aria-label={`${symbol} DarkPool balance`}
        className="text-body-md text-brand-fg"
      />
    </div>
  )
}

function amountFor(balances: Balances, symbol: TokenSymbol): string {
  return symbol === 'WETH' ? balances.weth : balances.usdc
}
