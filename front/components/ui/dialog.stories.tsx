import * as React from 'react'

import { Button } from './button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from './dialog'

export const Default = () => (
  <Dialog>
    <DialogTrigger asChild>
      <Button variant="ghost">[ DEPOSIT USDC ]</Button>
    </DialogTrigger>
    <DialogContent>
      <DialogHeader>
        <DialogTitle>DEPOSIT USDC</DialogTitle>
        <DialogDescription>
          Funds are escrowed inside the DarkPool contract until you withdraw.
        </DialogDescription>
      </DialogHeader>
      <p className="text-body-sm font-mono text-brand-muted">01 APPROVE → 02 DEPOSIT</p>
      <DialogFooter>
        <Button variant="ghost">Cancel</Button>
        <Button>Confirm</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
)

export const StartsOpen = () => {
  const [open, setOpen] = React.useState(true)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>WITHDRAW WETH</DialogTitle>
          <DialogDescription>Withdrawals settle in the next batch (~5s).</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button onClick={() => setOpen(false)}>Confirm</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
