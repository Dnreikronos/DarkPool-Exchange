'use client'

import { useEffect, useRef, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'

export function CommandPrompt() {
  const [open, setOpen] = useState(false)
  const triggerRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setOpen(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        ref={triggerRef}
        aria-label="Open command palette"
        className="group flex h-8 w-full items-center justify-between border-t border-brand-border px-page-x-mobile sm:px-page-x-tablet lg:px-page-x-desktop transition-colors duration-150 hover:bg-brand-surface focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent"
      >
        <span className="flex items-center gap-3 font-mono text-label-md uppercase text-brand-muted group-hover:text-brand-fg">
          <span aria-hidden="true" className="text-brand-fg">
            &gt;
          </span>
          <span>TYPE A COMMAND — PALETTE LANDS IN A FOLLOW-UP ISSUE</span>
        </span>
        <kbd className="font-mono text-label-md uppercase text-brand-muted">
          [ ⌘K ]
        </kbd>
      </DialogTrigger>
      <DialogContent className="max-w-md">
        <DialogTitle className="mb-2">COMMAND PALETTE</DialogTitle>
        <DialogDescription>
          Real Cmd-K behavior (jump-to-section, place-order shortcuts,
          wallet actions) lands in a follow-up issue. The shortcut is
          wired so the affordance is honest.
        </DialogDescription>
        <div className="mt-6 font-mono text-label-md uppercase text-brand-muted/70">
          [ TRACKED · TODO ]
        </div>
      </DialogContent>
    </Dialog>
  )
}
