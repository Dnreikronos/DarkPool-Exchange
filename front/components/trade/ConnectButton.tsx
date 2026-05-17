'use client'

import { useCallback, useEffect, useId, useRef, useState } from 'react'
import type { Address } from '@/lib/wallet'
import { useWallet } from '@/lib/wallet'

const MOCK_PROVIDERS = ['METAMASK', 'RAINBOW', 'WALLETCONNECT'] as const

const GHOST_BUTTON =
  'inline-flex h-12 items-center justify-center border border-brand-border2 px-8 ' +
  'font-mono text-[11px] font-medium uppercase tracking-[0.15em] text-brand-muted ' +
  'transition-colors duration-150 hover:border-brand-muted hover:text-white ' +
  'focus:outline-none focus-visible:border-brand-accent focus-visible:text-white'

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
      <div className="flex items-center gap-3">
        <span
          aria-label={`Connected as ${address}`}
          className="font-mono text-[11px] font-medium uppercase tracking-[0.15em] text-brand-muted"
        >
          [ {truncateAddress(address)} ]
        </span>
        <button type="button" onClick={disconnect} className={GHOST_BUTTON}>
          [ DISCONNECT ]
        </button>
      </div>
    )
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setPickerOpen(true)}
        aria-haspopup="dialog"
        aria-expanded={pickerOpen}
        className={GHOST_BUTTON}
      >
        [ CONNECT ]
      </button>
      {pickerOpen && <PickerModal onPick={handlePick} onClose={() => setPickerOpen(false)} />}
    </>
  )
}

interface PickerModalProps {
  onPick: () => void
  onClose: () => void
}

function PickerModal({ onPick, onClose }: PickerModalProps) {
  const titleId = useId()
  const dialogRef = useRef<HTMLDivElement>(null)
  const openerRef = useRef<Element | null>(null)
  const pointerDownOnBackdropRef = useRef(false)

  useEffect(() => {
    openerRef.current = document.activeElement
    const firstButton = dialogRef.current?.querySelector<HTMLButtonElement>('button')
    firstButton?.focus()

    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'

    return () => {
      document.body.style.overflow = previousOverflow
      if (openerRef.current instanceof HTMLElement) {
        openerRef.current.focus()
      }
    }
  }, [])

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.stopPropagation()
      onClose()
      return
    }
    if (event.key !== 'Tab') return
    const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])'
    )
    if (!focusable || focusable.length === 0) return
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    const active = document.activeElement
    if (event.shiftKey && active === first) {
      last.focus()
      event.preventDefault()
    } else if (!event.shiftKey && active === last) {
      first.focus()
      event.preventDefault()
    }
  }

  const handleBackdropPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    pointerDownOnBackdropRef.current = event.target === event.currentTarget
  }

  const handleBackdropClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const startedOnBackdrop = pointerDownOnBackdropRef.current
    pointerDownOnBackdropRef.current = false
    if (startedOnBackdrop && event.target === event.currentTarget) {
      onClose()
    }
  }

  return (
    <div
      onPointerDown={handleBackdropPointerDown}
      onClick={handleBackdropClick}
      onKeyDown={handleKeyDown}
      className="fixed inset-0 z-50 flex items-center justify-center bg-[rgba(6,6,10,0.85)] p-6"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="w-full max-w-sm bg-brand-surface p-8"
      >
        <h2
          id={titleId}
          className="mb-6 font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted"
        >
          [ CONNECT WALLET ]
        </h2>
        <ul className="flex flex-col gap-2">
          {MOCK_PROVIDERS.map((provider) => (
            <li key={provider}>
              <button
                type="button"
                onClick={onPick}
                className="w-full border border-brand-border px-6 py-4 text-left font-mono text-[11px] font-medium uppercase tracking-[0.15em] text-brand-muted transition-colors duration-150 hover:border-brand-border2 hover:text-white focus:outline-none focus-visible:border-brand-accent focus-visible:text-white"
              >
                [ {provider} ]
              </button>
            </li>
          ))}
        </ul>
        <p className="mt-6 font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted/60">
          MOCK · INJECTS 0X1111…1111
        </p>
      </div>
    </div>
  )
}
