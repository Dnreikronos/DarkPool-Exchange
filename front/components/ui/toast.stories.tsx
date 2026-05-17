import * as React from 'react'

import { Button } from './button'
import { Toaster } from './toaster'
import { useToast } from './use-toast'

const Trigger = () => {
  const { toast } = useToast()
  return (
    <div className="flex gap-3">
      <Button
        variant="ghost"
        onClick={() => toast({ title: 'Order placed', description: 'Pending next auction.' })}
      >
        [ ORDER PLACED ]
      </Button>
      <Button
        variant="ghost"
        onClick={() =>
          toast({
            title: 'Auction 1042 settled',
            description: '0.04 ETH @ 2,418.10 USDC',
            variant: 'accent',
          })
        }
      >
        [ AUCTION SETTLED ]
      </Button>
      <Button
        variant="ghost"
        onClick={() =>
          toast({
            title: 'Rejected by wallet',
            description: 'User cancelled the request.',
          })
        }
      >
        [ REJECTED ]
      </Button>
    </div>
  )
}

export const Default = () => (
  <>
    <Trigger />
    <Toaster />
  </>
)
