import * as React from 'react'

import { mockStore } from '@/lib/mock-store'
import { createDarkPoolClient } from '@/lib/sdk/client'
import { StoreMockClient } from '@/lib/sdk/mocks/client'
import { DarkPoolClientProvider } from '@/lib/sdk/provider'

import { Tape } from './Tape'

// Same singleton mock store the live tick feeds; binding the
// `StoreMockClient` to it lets the Tape's poll see every new auction the
// store pushes, so the story behaves like the runtime SDK path.
const storyClient = createDarkPoolClient({
  baseUrl: 'http://stories.local',
  apiKey: 'stories',
  useMocks: true,
  mockClient: new StoreMockClient({ store: mockStore }),
})

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
    <DarkPoolClientProvider client={storyClient}>
      <div className="flex h-[600px] w-[360px] flex-col border border-brand-border bg-brand-bg">
        <Tape />
      </div>
    </DarkPoolClientProvider>
  )
}

export const SmallLimit = () => {
  React.useEffect(() => {
    mockStore.getState().start({ perturbMs: 1000, auctionMs: 3000 })
    return () => mockStore.getState().stop()
  }, [])

  return (
    <DarkPoolClientProvider client={storyClient}>
      <div className="flex h-[400px] w-[360px] flex-col border border-brand-border bg-brand-bg">
        <Tape limit={5} />
      </div>
    </DarkPoolClientProvider>
  )
}
