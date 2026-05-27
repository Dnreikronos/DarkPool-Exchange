'use client'

import { ConnectButton as RainbowKitConnectButton } from '@rainbow-me/rainbowkit'

import { WalletIcon } from '@/app/app/_shell/icons'
import { Button } from '@/components/ui/button'

const HEADER_BUTTON_CLASS = 'h-10 px-4'

/**
 * Header connect button. Wraps RainbowKit's `ConnectButton.Custom` so
 * the chrome stays inside the DESIGN.md brutalist system (ghost
 * button, mono uppercase, no border radius), while RainbowKit handles
 * the wallet picker modal, EIP-1193 transport, and account
 * persistence.
 *
 * Wrong-network state is *not* surfaced here — `WrongNetworkModal`
 * (mounted inside `WalletProviders`) intercepts that case
 * application-wide.
 */
export function ConnectButton() {
  return (
    <RainbowKitConnectButton.Custom>
      {({ account, openAccountModal, openConnectModal, mounted }) => {
        const ready = mounted
        const connected = ready && Boolean(account)
        return (
          <div
            aria-hidden={!ready}
            style={{ opacity: ready ? 1 : 0, pointerEvents: ready ? 'auto' : 'none' }}
          >
            {connected && account ? (
              <Button
                variant="ghost"
                onClick={openAccountModal}
                aria-label={`Wallet ${account.address}`}
                className={HEADER_BUTTON_CLASS}
              >
                <WalletIcon className="mr-3 text-brand-fg" />
                {account.displayName}
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={openConnectModal}
                aria-haspopup="dialog"
                className={HEADER_BUTTON_CLASS}
              >
                <WalletIcon className="mr-3 text-brand-muted" />
                CONNECT
              </Button>
            )}
          </div>
        )
      }}
    </RainbowKitConnectButton.Custom>
  )
}
