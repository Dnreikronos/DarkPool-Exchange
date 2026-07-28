'use client'

// /trade composition root (#207). Mounts the real panels — OrderBook,
// charts, OrderEntry, Tape, MyOrders — into the desktop grid, the mobile
// stack and the mobile order-entry sheet, and wires click-to-fill from
// the book into the entry form via `OrderEntryHandle.fill`.
//
// The trading layout (app/app/layout.tsx → WalletProviders) already
// provides a shared QueryClientProvider + DarkPoolClientProvider, so the
// provider-less `OrderBookContent` / `TapeContent` variants are used here
// to share one TanStack Query cache instead of the panels' scoped clients.

import {
  useCallback,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
  type Ref,
  type SVGProps,
} from 'react'
import { ChartGlyph, OrderbookGlyph, TapeGlyph } from '@/app/app/_shell/icons'
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from '@/components/ui/sheet'
import { Side } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb'

import { DepthChart, PriceHistoryChart } from './charts'
import { OrderEntry, type OrderEntryHandle, type OrderSide } from './entry'
import { MyOrdersPanel } from './my-orders'
import { OrderBookContent } from './orderbook'
import { TapeContent } from './tape'

type Glyph = ComponentType<SVGProps<SVGSVGElement>>
type PriceSelectHandler = (price: string, side: Side.BUY | Side.SELL) => void

/** Book rows pass the proto enum; the form speaks `'buy' | 'sell'`. */
function toOrderSide(side: Side.BUY | Side.SELL): OrderSide {
  return side === Side.BUY ? 'buy' : 'sell'
}

export function Shell() {
  // Desktop click-to-fill: book row → the grid's order-entry form.
  const desktopEntryRef = useRef<OrderEntryHandle>(null)
  const handleDesktopPriceSelect = useCallback<PriceSelectHandler>((price, side) => {
    desktopEntryRef.current?.fill(price, toOrderSide(side))
  }, [])

  // Mobile click-to-fill: the sheet's form only exists while the sheet is
  // open (Radix mounts content on demand), so a tap on a book row opens
  // the sheet and the fill is applied when the form's handle attaches.
  const [sheetOpen, setSheetOpen] = useState(false)
  const sheetEntryHandleRef = useRef<OrderEntryHandle | null>(null)
  const pendingFillRef = useRef<{ price: string; side: OrderSide } | null>(null)

  const sheetEntryRef = useCallback((handle: OrderEntryHandle | null) => {
    sheetEntryHandleRef.current = handle
    if (handle && pendingFillRef.current) {
      handle.fill(pendingFillRef.current.price, pendingFillRef.current.side)
      pendingFillRef.current = null
    }
  }, [])

  const handleMobilePriceSelect = useCallback<PriceSelectHandler>((price, side) => {
    const fill = { price, side: toOrderSide(side) }
    if (sheetEntryHandleRef.current) {
      sheetEntryHandleRef.current.fill(fill.price, fill.side)
    } else {
      pendingFillRef.current = fill
    }
    setSheetOpen(true)
  }, [])

  return (
    <>
      <DesktopLayout entryRef={desktopEntryRef} onPriceSelect={handleDesktopPriceSelect} />
      <MobileLayout onPriceSelect={handleMobilePriceSelect} />
      <MobileOrderEntryDock open={sheetOpen} onOpenChange={setSheetOpen} entryRef={sheetEntryRef} />
    </>
  )
}

function DesktopLayout({
  entryRef,
  onPriceSelect,
}: {
  entryRef: Ref<OrderEntryHandle>
  onPriceSelect: PriceSelectHandler
}) {
  return (
    <div className="hidden lg:flex lg:min-h-[calc(100vh-4rem)] lg:flex-col">
      <div className="grid flex-1 grid-cols-[1fr_2fr_1fr]">
        <section aria-label="Order book" className="min-h-0 border-r border-brand-border">
          <OrderbookPanel onPriceSelect={onPriceSelect} />
        </section>
        <section
          aria-label="Market chart and order entry"
          className="grid grid-rows-[2fr_1fr] border-r border-brand-border"
        >
          <div className="min-h-0 overflow-hidden">
            <ChartPanel />
          </div>
          <div className="min-h-0 overflow-y-auto border-t border-brand-border">
            <OrderEntry ref={entryRef} />
          </div>
        </section>
        <section aria-label="Auction tape" className="min-h-0">
          <TapePanel />
        </section>
      </div>
      <section aria-label="My orders" className="border-t border-brand-border">
        <MyOrdersPanel />
      </section>
    </div>
  )
}

function MobileLayout({ onPriceSelect }: { onPriceSelect: PriceSelectHandler }) {
  return (
    <div className="lg:hidden flex flex-col pb-20">
      <section aria-label="Order book" className="border-b border-brand-border">
        <OrderbookPanel onPriceSelect={onPriceSelect} />
      </section>
      <section aria-label="Market chart" className="border-b border-brand-border">
        <ChartPanel />
      </section>
      <section aria-label="Auction tape" className="border-b border-brand-border">
        <TapePanel />
      </section>
      <section aria-label="My orders">
        <MyOrdersPanel />
      </section>
    </div>
  )
}

function MobileOrderEntryDock({
  open,
  onOpenChange,
  entryRef,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  entryRef: Ref<OrderEntryHandle>
}) {
  return (
    <div className="lg:hidden fixed inset-x-0 bottom-0 z-30 border-t border-brand-border bg-brand-bg/95 backdrop-blur-sm px-4 py-3">
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetTrigger
          className="block w-full bg-brand-accent px-6 py-3 text-center font-mono text-label-lg uppercase text-brand-on-accent transition-shadow duration-150 hover:shadow-accent-glow focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
          aria-label="Open order entry"
        >
          NEW ORDER
        </SheetTrigger>
        <SheetContent side="right" className="flex w-full flex-col sm:max-w-md">
          {/* OrderEntry carries its own visible "[ ORDER ENTRY ]" header —
              the dialog title stays for assistive tech only. */}
          <SheetTitle className="sr-only">ORDER ENTRY</SheetTitle>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <OrderEntry ref={entryRef} />
          </div>
        </SheetContent>
      </Sheet>
    </div>
  )
}

function Panel({
  label,
  icon: Icon,
  children,
}: {
  label: string
  icon: Glyph
  children: ReactNode
}) {
  return (
    <div className="flex h-full min-h-[200px] flex-col">
      <PanelHeader label={label} icon={Icon} />
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
    </div>
  )
}

function PanelHeader({ label, icon: Icon }: { label: string; icon: Glyph }) {
  return (
    <div className="flex h-9 items-center gap-3 border-b border-brand-border px-4">
      <Icon className="text-brand-muted" />
      <span className="font-mono text-label-md uppercase text-brand-muted">{label}</span>
    </div>
  )
}

function OrderbookPanel({ onPriceSelect }: { onPriceSelect: PriceSelectHandler }) {
  return (
    <Panel label="ORDERBOOK · ETH / USDC" icon={OrderbookGlyph}>
      <OrderBookContent onPriceSelect={onPriceSelect} />
    </Panel>
  )
}

function ChartPanel() {
  return (
    <Panel label="MARKET · ETH / USDC" icon={ChartGlyph}>
      <PriceHistoryChart className="min-h-0 flex-[3]" />
      <DepthChart className="min-h-0 flex-[2] border-t border-brand-border" />
    </Panel>
  )
}

function TapePanel() {
  return (
    <Panel label="AUCTION TAPE" icon={TapeGlyph}>
      <TapeContent />
    </Panel>
  )
}
