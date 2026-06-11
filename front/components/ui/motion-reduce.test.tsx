// @vitest-environment jsdom

// Locks the prefers-reduced-motion contract on the shared primitives
// (#80): every Radix entrance/exit animation carries a motion-reduce
// variant matching the data-[state] selector's specificity (a bare
// `motion-reduce:animate-none` loses the cascade against
// `data-[state=open]:animate-*`). Pattern reference: StatusPill /
// skeleton use plain `animate-* motion-reduce:animate-none`.

import * as React from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import { Dialog, DialogContent, DialogDescription, DialogTitle } from './dialog'
import { Sheet, SheetContent, SheetTitle } from './sheet'
import { Toast, ToastProvider, ToastViewport } from './toast'
import { PlaceButton } from '@/app/app/trade/_components/entry/ProveSubmitStages'

describe('reduced-motion variants on shared primitives', () => {
  afterEach(cleanup)

  it('dialog overlay and content freeze under reduced motion', () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>T</DialogTitle>
          <DialogDescription>D</DialogDescription>
        </DialogContent>
      </Dialog>
    )
    const dialog = screen.getByRole('dialog')
    expect(dialog.className).toContain('motion-reduce:data-[state=open]:animate-none')
    const overlay = document.querySelector('[class*="fixed inset-0"]')
    expect(overlay?.className).toContain('motion-reduce:data-[state=open]:animate-none')
  })

  it('sheet content freezes under reduced motion', () => {
    render(
      <Sheet open>
        <SheetContent>
          <SheetTitle>T</SheetTitle>
        </SheetContent>
      </Sheet>
    )
    expect(screen.getByRole('dialog').className).toContain(
      'motion-reduce:data-[state=open]:animate-none'
    )
  })

  it('toast freezes under reduced motion', () => {
    const { container } = render(
      <ToastProvider>
        <Toast open>t</Toast>
        <ToastViewport />
      </ToastProvider>
    )
    // The toast root is the <li data-state="open"> inside the viewport
    // (role="status" lands on Radix's internal announce region instead).
    const root = container.querySelector('li[data-state]')
    expect(root?.className).toContain('motion-reduce:data-[state=open]:animate-none')
    expect(root?.className).toContain('motion-reduce:data-[swipe=end]:animate-none')
  })

  it('place-button progress bar freezes under reduced motion', () => {
    render(
      <PlaceButton
        idleLabel="BUY · WETH"
        phase={{ kind: 'running', stage: 'proving', progress: 0.4 }}
        onClick={() => {}}
        accent={false}
      />
    )
    const bar = screen.getByTestId('place-progress')
    expect(bar.className).toContain('motion-reduce:transition-none')
    expect(bar.parentElement?.className).toContain('motion-reduce:transition-none')
  })
})
