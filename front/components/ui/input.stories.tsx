import * as React from 'react'

import { Input } from './input'

export const Default = () => (
  <div className="flex flex-col gap-2 max-w-xs">
    <label
      className="text-label-md font-mono uppercase tracking-[0.2em] text-brand-muted"
      htmlFor="price"
    >
      [ PRICE · USDC ]
    </label>
    <Input id="price" placeholder="0.00" defaultValue="2418.10" />
  </div>
)

export const WithSize = () => (
  <div className="flex flex-col gap-2 max-w-xs">
    <label
      className="text-label-md font-mono uppercase tracking-[0.2em] text-brand-muted"
      htmlFor="size"
    >
      [ SIZE · WETH ]
    </label>
    <Input id="size" placeholder="0.0000" />
  </div>
)

export const Disabled = () => (
  <div className="flex flex-col gap-2 max-w-xs">
    <label
      className="text-label-md font-mono uppercase tracking-[0.2em] text-brand-muted"
      htmlFor="locked"
    >
      [ MARKET ]
    </label>
    <Input id="locked" defaultValue="ETH / USDC" disabled />
  </div>
)
