'use client'

import { useCallback, useState } from 'react'
import { WalletIcon } from '@/app/app/_shell/icons'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import type { Address } from '@/lib/wallet'
import { useWallet } from '@/lib/wallet'

const MOCK_PROVIDERS = ['METAMASK', 'RAINBOW', 'WALLETCONNECT'] as const

const HEADER_BUTTON_CLASS = 'h-10 px-4'

function truncateAddress(address: Address): string {
  return `${address.slice(0, 6)}…${address.slice(-4)}`
}

export function ConnectButton() {
  const { isConnected, address, connect, disconnect } = useWallet()
  const [pickerOpen, setPickerOpen] = useState(false)

  const handlePick = useCallback(() => {
    connect()
    setPickerOpen(false)
  }, [connect])

  if (isConnected && address) {
    return (
      <Button
        variant="ghost"
        onClick={disconnect}
        aria-label={`Disconnect wallet ${address}`}
        className={HEADER_BUTTON_CLASS}
      >
        <WalletIcon className="mr-3 text-brand-fg" />
        {truncateAddress(address)}
      </Button>
    )
  }

  return (
    <>
      <Button
        variant="ghost"
        onClick={() => setPickerOpen(true)}
        aria-haspopup="dialog"
        aria-expanded={pickerOpen}
        className={HEADER_BUTTON_CLASS}
      >
        <WalletIcon className="mr-3 text-brand-muted" />
        CONNECT
      </Button>
      <Dialog open={pickerOpen} onOpenChange={setPickerOpen}>
        <DialogContent className="max-w-sm">
          <DialogTitle className="mb-6 !font-mono !text-label-md !uppercase !tracking-[0.2em] !text-brand-muted">
            CONNECT WALLET
          </DialogTitle>
          <ul className="flex flex-col gap-2">
            {MOCK_PROVIDERS.map((provider) => (
              <li key={provider}>
                <Button variant="ghost" onClick={handlePick} className="w-full justify-start">
                  {provider}
                </Button>
              </li>
            ))}
          </ul>
          <p className="mt-6 font-mono text-label-md uppercase text-brand-muted/60">
            MOCK · INJECTS 0X1111…1111
          </p>
        </DialogContent>
      </Dialog>
    </>
  )
}
