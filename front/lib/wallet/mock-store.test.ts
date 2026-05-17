import { beforeEach, describe, expect, it } from 'vitest'
import { MOCK_ADDRESS, walletStore } from './mock-store'

describe('walletStore', () => {
  beforeEach(() => {
    walletStore.disconnect()
  })

  it('starts disconnected with zeroed balances', () => {
    const state = walletStore.getState()
    expect(state.status).toBe('disconnected')
    expect(state.address).toBeNull()
    expect(state.walletBalances).toEqual({ weth: '0', usdc: '0' })
    expect(state.internalBalances).toEqual({ weth: '0', usdc: '0' })
  })

  it('connect injects MOCK_ADDRESS and seeds the AC balances (1 WETH / 1000 USDC wallet, 0/0 internal)', () => {
    walletStore.connect()
    const state = walletStore.getState()
    expect(state.status).toBe('connected')
    expect(state.address).toBe(MOCK_ADDRESS)
    expect(state.walletBalances).toEqual({ weth: '1', usdc: '1000' })
    expect(state.internalBalances).toEqual({ weth: '0', usdc: '0' })
  })

  it('disconnect resets address and balances', () => {
    walletStore.connect()
    walletStore.disconnect()
    const state = walletStore.getState()
    expect(state.status).toBe('disconnected')
    expect(state.address).toBeNull()
    expect(state.walletBalances).toEqual({ weth: '0', usdc: '0' })
    expect(state.internalBalances).toEqual({ weth: '0', usdc: '0' })
  })

  it('notifies subscribers on connect and disconnect transitions', () => {
    let fires = 0
    const unsubscribe = walletStore.subscribe(() => {
      fires += 1
    })

    walletStore.connect()
    expect(fires).toBe(1)

    walletStore.disconnect()
    expect(fires).toBe(2)

    unsubscribe()
    walletStore.connect()
    expect(fires).toBe(2)
  })

  it('connect is idempotent: a second connect does not re-notify', () => {
    walletStore.connect()
    let fires = 0
    const unsubscribe = walletStore.subscribe(() => {
      fires += 1
    })
    walletStore.connect()
    expect(fires).toBe(0)
    unsubscribe()
  })

  it('disconnect is idempotent when already disconnected', () => {
    let fires = 0
    const unsubscribe = walletStore.subscribe(() => {
      fires += 1
    })
    walletStore.disconnect()
    expect(fires).toBe(0)
    unsubscribe()
  })
})
