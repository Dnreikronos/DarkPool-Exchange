'use client'

import * as React from 'react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import type { AuctionSummary } from '@/lib/sdk'
import { shortTxHash, type SettlementLink } from '@/lib/settlement'

import { formatFullTimestamp } from '../../_lib/tape/format'
import styles from './tape.module.css'

export interface TapeDrawerProps {
  auction: AuctionSummary | null
  /**
   * On-chain settlement correlated to this auction (#100). Null until a
   * BatchSettled event lands within the correlation window.
   */
  link?: SettlementLink | null
  onClose: () => void
}

export function TapeDrawer({ auction, link = null, onClose }: TapeDrawerProps): JSX.Element {
  const open = auction !== null
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      <DialogContent
        className={`${styles.drawer} fixed left-auto right-0 top-0 h-screen w-full max-w-[420px] translate-x-0 translate-y-0 border-l border-brand-border bg-brand-surface p-8`}
      >
        {auction && (
          <>
            <DialogTitle className="font-display text-display-sm uppercase">
              [ AUCTION {auction.auctionId} ]
            </DialogTitle>
            <DialogDescription className="mt-2 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
              {auction.pair}
            </DialogDescription>

            <dl className="mt-8 grid grid-cols-[max-content_minmax(0,1fr)] gap-y-3 font-mono text-body-sm">
              <Row label="TIMESTAMP" value={formatFullTimestamp(auction.timestampUnix)} mono />
              <Row label="CLEARING" value={auction.clearingPrice} mono />
              <Row label="VOLUME" value={auction.matchedVolume} mono />
              <Row label="MATCHES" value={String(auction.matchCount)} mono />
            </dl>

            <div className="mt-10 flex flex-col gap-2">
              <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
                ETHERSCAN
              </span>
              <SettlementReceipt link={link} />
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

function SettlementReceipt({ link }: { link: SettlementLink | null }): JSX.Element {
  if (!link) {
    return (
      <span
        aria-label="Etherscan link pending settlement"
        className="font-mono text-label-lg uppercase tracking-label text-brand-muted"
      >
        [ ETHERSCAN · PENDING ]
      </span>
    )
  }
  if (!link.url) {
    // Local devnets have no block explorer — surface the hash as text.
    return (
      <span
        title={link.txHash}
        className="font-mono text-label-lg tracking-label text-brand-fg tabular-nums"
      >
        [ {shortTxHash(link.txHash)} ]
      </span>
    )
  }
  return (
    <a
      href={link.url}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={`View settlement transaction ${link.txHash} on the block explorer`}
      title={link.txHash}
      className="font-mono text-label-lg tracking-label text-brand-fg tabular-nums underline decoration-brand-border underline-offset-4 transition-colors hover:decoration-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
    >
      [ {shortTxHash(link.txHash)} ]
    </a>
  )
}

function Row({
  label,
  value,
  mono,
}: {
  label: string
  value: string
  mono?: boolean
}): JSX.Element {
  return (
    <>
      <dt className="pr-4 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {label}
      </dt>
      <dd className={`${mono ? 'tabular-nums' : ''} text-brand-fg`}>{value}</dd>
    </>
  )
}
