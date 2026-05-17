'use client'

import * as React from 'react'

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import type { AuctionSummary } from '@/lib/sdk'

import { formatFullTimestamp } from './format'
import styles from './tape.module.css'

export interface TapeDrawerProps {
  auction: AuctionSummary | null
  onClose: () => void
}

export function TapeDrawer({ auction, onClose }: TapeDrawerProps): JSX.Element {
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
        aria-describedby={undefined}
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
              <span
                aria-label="Etherscan link pending Phase 2 integration"
                className="font-mono text-label-lg uppercase tracking-label text-brand-muted"
              >
                [ ETHERSCAN · PENDING ]
              </span>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }): JSX.Element {
  return (
    <>
      <dt className="pr-4 font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {label}
      </dt>
      <dd className={`${mono ? 'tabular-nums' : ''} text-brand-fg`}>{value}</dd>
    </>
  )
}
