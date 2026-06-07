'use client'

import { useEffect, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'

export function CommandPrompt() {
  const [open, setOpen] = useState(false)

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
        aria-label="Open command palette"
        className="flex h-9 w-full items-center justify-between px-4 font-mono text-label-md uppercase text-brand-muted transition-colors duration-150 hover:bg-brand-surface hover:text-brand-fg focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent"
      >
        <span>COMMAND</span>
        <kbd className="font-mono text-label-md uppercase">⌘K</kbd>
      </DialogTrigger>
      <DialogContent className="max-w-md">
        <DialogTitle className="mb-2">COMMAND PALETTE</DialogTitle>
        <DialogDescription>
          Real Cmd-K behavior (jump-to-section, place-order shortcuts, wallet actions) lands in a
          follow-up issue. The shortcut is wired so the affordance is honest.
        </DialogDescription>
        {/* Full-strength muted: brand-muted is already ~3:1 on the canvas —
            a /70 alpha pushes ambient metadata below any readable floor (#80). */}
        <div className="mt-6 font-mono text-label-md uppercase text-brand-muted">
          [ TRACKED · TODO ]
        </div>
      </DialogContent>
    </Dialog>
  )
}
