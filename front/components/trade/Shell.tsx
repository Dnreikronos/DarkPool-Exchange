'use client'

import { type ReactNode } from 'react'
import {
  Sheet,
  SheetContent,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'

export function Shell() {
  return (
    <>
      <DesktopLayout />
      <MobileLayout />
      <MobileOrderEntryDock />
    </>
  )
}

function DesktopLayout() {
  return (
    <div className="hidden lg:grid lg:grid-cols-[1fr_2fr_1fr] lg:min-h-[calc(100vh-4rem)]">
      <section
        aria-label="Order book"
        className="border-r border-brand-border"
      >
        <OrderbookPanel />
      </section>
      <section
        aria-label="Market chart and order entry"
        className="grid grid-rows-[2fr_1fr] border-r border-brand-border"
      >
        <ChartPanel />
        <div className="border-t border-brand-border">
          <OrderEntryPanel />
        </div>
      </section>
      <section aria-label="Auction tape">
        <TapePanel />
      </section>
    </div>
  )
}

function MobileLayout() {
  return (
    <div className="lg:hidden flex flex-col pb-20">
      <section aria-label="Order book" className="border-b border-brand-border">
        <OrderbookPanel />
      </section>
      <section
        aria-label="Market chart"
        className="border-b border-brand-border"
      >
        <ChartPanel />
      </section>
      <section aria-label="Auction tape">
        <TapePanel />
      </section>
    </div>
  )
}

function MobileOrderEntryDock() {
  return (
    <div className="lg:hidden fixed inset-x-0 bottom-0 z-30 border-t border-brand-border bg-brand-bg/95 backdrop-blur-sm px-4 py-3">
      <Sheet>
        <SheetTrigger
          className="block w-full border border-brand-border2 px-6 py-3 text-center font-mono text-label-lg uppercase text-brand-muted transition-colors duration-150 hover:border-brand-muted hover:text-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
          aria-label="Open order entry"
        >
          [ NEW ORDER ]
        </SheetTrigger>
        <SheetContent side="right" className="w-full sm:max-w-md">
          <SheetTitle className="mb-4">ORDER ENTRY</SheetTitle>
          <p className="font-mono text-body-md text-brand-muted">
            Awaiting wallet connection and order entry form (F1.9).
          </p>
        </SheetContent>
      </Sheet>
    </div>
  )
}

function Panel({
  label,
  empty,
  children,
}: {
  label: string
  empty?: string
  children?: ReactNode
}) {
  return (
    <div className="flex h-full min-h-[200px] flex-col">
      <PanelHeader label={label} />
      <div className="flex flex-1 items-center justify-center p-page-x-mobile">
        {children ?? <EmptyState label={empty ?? 'AWAITING DATA'} />}
      </div>
    </div>
  )
}

function PanelHeader({ label }: { label: string }) {
  return (
    <div className="flex h-9 items-center justify-between border-b border-brand-border bg-brand-surface px-4">
      <span className="font-mono text-label-md uppercase text-brand-muted">
        [ {label} ]
      </span>
    </div>
  )
}

function EmptyState({ label }: { label: string }) {
  return (
    <p
      role="status"
      className="font-mono text-label-md uppercase text-brand-muted"
    >
      [ {label} ]
    </p>
  )
}

function OrderbookPanel() {
  return <Panel label="ORDERBOOK · ETH / USDC" empty="NO DATA · F1.6" />
}

function ChartPanel() {
  return <Panel label="MARKET · ETH / USDC" empty="CHART · F1.8" />
}

function OrderEntryPanel() {
  return <Panel label="ORDER ENTRY" empty="AWAITING WALLET · F1.9" />
}

function TapePanel() {
  return <Panel label="AUCTION TAPE" empty="NO AUCTIONS YET · F1.7" />
}
