// @vitest-environment jsdom

// Automated axe-core scan of the /trade shell (#80). jsdom applies no
// stylesheets, so BOTH the desktop and mobile layouts are present in the
// DOM at once — any duplicate-ID finding here is real, not an artifact.
// Also scans with the mobile order-entry sheet open (Radix portal).

import * as React from 'react'
import { afterEach, describe, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import { walletStore } from '@/lib/wallet/mock-store'
import { expectNoAxeViolations } from '@/test/axe'

import { Shell } from './Shell'

describe('Shell a11y', () => {
  afterEach(() => {
    walletStore.disconnect()
    cleanup()
  })

  it('has no axe violations when disconnected', async () => {
    render(<Shell />)
    await expectNoAxeViolations()
  })

  it('has no axe violations when connected', async () => {
    walletStore.connect()
    render(<Shell />)
    await expectNoAxeViolations()
  })

  it('has no axe violations with the mobile order-entry sheet open', async () => {
    render(<Shell />)
    fireEvent.click(screen.getByRole('button', { name: 'Open order entry' }))
    await expectNoAxeViolations()
  })
})
