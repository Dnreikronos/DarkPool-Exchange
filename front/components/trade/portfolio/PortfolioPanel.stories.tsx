import * as React from 'react'

import { mockStore } from '../../../lib/mock-store'
import { Side } from '../../../lib/sdk/proto/darkpool/v1/darkpool_pb'
import { walletStore } from '../../../lib/wallet/mock-store'

import { PortfolioPanel } from './PortfolioPanel'

// Ladle stories for visual verification. Each story imperatively positions
// the mock + wallet stores before render; Ladle isolates stories in their
// own iframe so per-story side effects don't leak across the gallery.

function useDisconnectedOnMount() {
  React.useEffect(() => {
    walletStore.disconnect()
    return () => walletStore.disconnect()
  }, [])
}

function useConnectedOnMount() {
  React.useEffect(() => {
    walletStore.connect()
    return () => walletStore.disconnect()
  }, [])
}

function useSeededFills(args: { buys: number; sells: number }) {
  React.useEffect(() => {
    const s = mockStore.getState()
    s.stop()
    s.reset()
    for (let i = 0; i < args.buys; i++) {
      s.placeOrder({ side: Side.BUY, price: '3000', size: '0.25' })
      s.runAuction()
    }
    for (let i = 0; i < args.sells; i++) {
      s.placeOrder({ side: Side.SELL, price: '3050', size: '0.25' })
      s.runAuction()
    }
    return () => {
      s.stop()
    }
  }, [args.buys, args.sells])
}

function useLiveTicks() {
  React.useEffect(() => {
    const s = mockStore.getState()
    s.start({ perturbMs: 1000, auctionMs: 4000 })
    return () => s.stop()
  }, [])
}

export const Disconnected = () => {
  useDisconnectedOnMount()
  return <PortfolioPanel />
}

export const EmptyConnected = () => {
  useConnectedOnMount()
  React.useEffect(() => {
    mockStore.getState().reset()
  }, [])
  return <PortfolioPanel />
}

export const WithFills = () => {
  useConnectedOnMount()
  useSeededFills({ buys: 3, sells: 1 })
  return <PortfolioPanel />
}

export const FlatRoundTrip = () => {
  useConnectedOnMount()
  // Same buys/sells size — net WETH flat, USDC delta = realized P&L.
  useSeededFills({ buys: 2, sells: 2 })
  return <PortfolioPanel />
}

export const LiveAuctions = () => {
  // Watch the panel react as the mock auction tick consumes orders.
  useConnectedOnMount()
  React.useEffect(() => {
    const s = mockStore.getState()
    s.reset()
    // Seed a couple of standing orders so the live auctions have something to fill.
    s.placeOrder({ side: Side.BUY, price: '3010', size: '0.5' })
    s.placeOrder({ side: Side.BUY, price: '2990', size: '0.5' })
    s.placeOrder({ side: Side.SELL, price: '3020', size: '0.5' })
  }, [])
  useLiveTicks()
  return <PortfolioPanel />
}
