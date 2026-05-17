import * as React from 'react'

import { Button } from './button'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from './sheet'

export const Default = () => (
  <Sheet>
    <SheetTrigger asChild>
      <Button variant="ghost">[ AUCTION DETAIL ]</Button>
    </SheetTrigger>
    <SheetContent side="right">
      <SheetHeader>
        <SheetTitle>AUCTION 1042</SheetTitle>
        <SheetDescription>
          Cleared 0.04 ETH @ 2,418.10 USDC across 3 matched orders.
        </SheetDescription>
      </SheetHeader>
      <div className="font-mono text-[11px] uppercase tracking-[0.15em] text-brand-muted">
        [ TX 0x4f3a… ARBITRUM ]
      </div>
    </SheetContent>
  </Sheet>
)

export const FromLeft = () => (
  <Sheet>
    <SheetTrigger asChild>
      <Button variant="ghost">[ ORDERBOOK ]</Button>
    </SheetTrigger>
    <SheetContent side="left">
      <SheetHeader>
        <SheetTitle>ORDERBOOK</SheetTitle>
        <SheetDescription>Narrow viewport drawer placement.</SheetDescription>
      </SheetHeader>
    </SheetContent>
  </Sheet>
)
