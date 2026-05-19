import * as React from 'react'

import { mockStore } from '@/lib/mock-store'

import { Tape } from './Tape'

/**
 * Renders the live tape against the singleton mock store, starting and
 * stopping its tick loop on mount/unmount. Reload the story to reseed.
 */
export const Live = () => {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 5000 })
    return () => mockStore.getState().stop()
  }, [])

  return (
    <div className="flex h-[600px] w-[360px] flex-col border border-brand-border bg-brand-bg">
      <Tape />
    </div>
  )
}

export const SmallLimit = () => {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 3000 })
    return () => mockStore.getState().stop()
  }, [])

  return (
    <div className="flex h-[400px] w-[360px] flex-col border border-brand-border bg-brand-bg">
      <Tape limit={5} />
    </div>
  )
}
