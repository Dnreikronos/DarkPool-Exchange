import * as React from 'react'

import { Button } from './button'

export const Primary = () => <Button>Place Order</Button>
export const Ghost = () => <Button variant="ghost">Cancel</Button>
export const PrimaryDisabled = () => <Button disabled>Place Order</Button>
export const GhostDisabled = () => (
  <Button variant="ghost" disabled>
    Cancel
  </Button>
)
export const SmallGhost = () => (
  <Button variant="ghost" size="sm">
    Max
  </Button>
)

export const Pair = () => (
  <div className="flex items-center gap-3">
    <Button variant="ghost">Cancel</Button>
    <Button>Place Order</Button>
  </div>
)
