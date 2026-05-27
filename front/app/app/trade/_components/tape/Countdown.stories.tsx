import * as React from 'react'

import { Countdown } from './Countdown'

const NOW = 1_700_000_000

export const Waiting = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={null} nowUnixSeconds={NOW} />
  </div>
)

export const JustAfterAuction = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW)} nowUnixSeconds={NOW} />
  </div>
)

export const HalfwayToNext = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW - 2)} nowUnixSeconds={NOW} />
  </div>
)

export const OneSecondLeft = () => (
  <div className="w-[320px]">
    <Countdown latestAuctionUnixSeconds={BigInt(NOW - 4)} nowUnixSeconds={NOW} />
  </div>
)
