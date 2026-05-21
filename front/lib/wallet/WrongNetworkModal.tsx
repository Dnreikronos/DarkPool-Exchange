'use client'

import { useAccount, useChainId, useSwitchChain } from 'wagmi'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

import { targetChain } from './wagmi-config'

/**
 * Blocking modal shown when the connected wallet is on a chain that
 * is not the one this app was built against (config.chainId).
 *
 * The Dialog has no close affordance (`onOpenChange` ignores attempts
 * to close it) — the only way out is `switchChain`, which fires the
 * wallet's network-switch prompt. If the user rejects, we stay open.
 */
export function WrongNetworkModal() {
  const { isConnected } = useAccount()
  const currentChainId = useChainId()
  const { switchChain, isPending } = useSwitchChain()

  const mismatched = isConnected && currentChainId !== targetChain.id
  if (!mismatched) return null

  return (
    <Dialog open onOpenChange={() => undefined}>
      <DialogContent
        className="max-w-md"
        // Prevent the Radix "click outside / escape" exits — wrong
        // network is a blocking condition.
        onPointerDownOutside={(event) => event.preventDefault()}
        onEscapeKeyDown={(event) => event.preventDefault()}
        // Hide the default close button injected by DialogContent.
        // The user has exactly one path forward: switchChain.
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogTitle className="mb-2">WRONG NETWORK</DialogTitle>
        <DialogDescription className="mb-6">
          DarkPool runs on{' '}
          <span className="text-brand-fg">
            {targetChain.name.toUpperCase()} · {targetChain.id}
          </span>
          . Your wallet is on chain {currentChainId}. Switch to continue.
        </DialogDescription>
        <Button
          variant="primary"
          onClick={() => switchChain({ chainId: targetChain.id })}
          disabled={isPending}
          className="w-full"
        >
          {isPending ? 'SWITCHING…' : `SWITCH TO ${targetChain.name.toUpperCase()}`}
        </Button>
      </DialogContent>
    </Dialog>
  )
}
