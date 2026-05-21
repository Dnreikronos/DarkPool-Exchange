'use client'

import * as DialogPrimitive from '@radix-ui/react-dialog'
import { useAccount, useChainId, useSwitchChain } from 'wagmi'

import { Button } from '@/components/ui/button'
import { cn } from '@/components/ui/cn'

import { targetChain } from './wagmi-config'

/**
 * Blocking modal shown when the connected wallet is on a chain that
 * is not the one this app was built against (config.chainId).
 *
 * This deliberately bypasses the shared `DialogContent` wrapper from
 * components/ui/dialog because that wrapper injects an always-on X
 * close button — rendering a close affordance that does nothing
 * (because `onOpenChange` is a no-op) would be a misleading UX in a
 * blocking flow. We use Radix primitives directly so the only path
 * forward is the explicit `switchChain` button.
 */
export function WrongNetworkModal() {
  const { isConnected } = useAccount()
  const currentChainId = useChainId()
  const { switchChain, isPending } = useSwitchChain()

  const mismatched = isConnected && currentChainId !== targetChain.id
  if (!mismatched) return null

  return (
    <DialogPrimitive.Root open>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          className={cn(
            'fixed inset-0 z-50 bg-[rgba(6,6,10,0.85)]',
            'data-[state=open]:animate-fade-in'
          )}
        />
        <DialogPrimitive.Content
          className={cn(
            'fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2',
            'w-full max-w-md bg-brand-surface text-brand-fg p-8',
            'data-[state=open]:animate-fade-in',
            'focus:outline-none'
          )}
          onPointerDownOutside={(event) => event.preventDefault()}
          onEscapeKeyDown={(event) => event.preventDefault()}
          onInteractOutside={(event) => event.preventDefault()}
        >
          <DialogPrimitive.Title className="mb-2 font-display text-[24px] uppercase leading-none text-brand-fg">
            WRONG NETWORK
          </DialogPrimitive.Title>
          <DialogPrimitive.Description className="mb-6 font-mono text-[12px] leading-[1.8] text-brand-muted">
            DarkPool runs on{' '}
            <span className="text-brand-fg">
              {targetChain.name.toUpperCase()} · {targetChain.id}
            </span>
            . Your wallet is on chain {currentChainId}. Switch to continue.
          </DialogPrimitive.Description>
          <Button
            variant="primary"
            onClick={() => switchChain({ chainId: targetChain.id })}
            disabled={isPending}
            className="w-full"
          >
            {isPending ? 'SWITCHING…' : `SWITCH TO ${targetChain.name.toUpperCase()}`}
          </Button>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
