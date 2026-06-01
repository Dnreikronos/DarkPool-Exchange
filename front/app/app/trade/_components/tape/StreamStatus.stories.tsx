import * as React from 'react'

import { StreamStatus } from './StreamStatus'

export const Live = () => (
  <div className="w-[320px] bg-brand-bg p-4">
    <StreamStatus status="live" />
  </div>
)

export const Connecting = () => (
  <div className="w-[320px] bg-brand-bg p-4">
    <StreamStatus status="connecting" />
  </div>
)

export const Degraded = () => (
  <div className="w-[320px] bg-brand-bg p-4">
    <StreamStatus status="degraded" />
  </div>
)
