import { beforeEach, describe, expect, it } from 'vitest'
import { MOCK_ADDRESS, walletStore } from './mock-store'

describe('walletStore', () => {
  beforeEach(() => {
    walletStore.disconnect()
    walletStore.resetTxState()
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

  describe('tx state (F1.5)', () => {
    it('defaults to unpaused with zero allowances', () => {
      const tx = walletStore.getTxState()
      expect(tx.paused).toBe(false)
      expect(tx.allowances).toEqual({ weth: '0', usdc: '0' })
    })

    it('setPaused flips the flag and notifies', () => {
      let fires = 0
      const unsubscribe = walletStore.subscribe(() => {
        fires += 1
      })
      walletStore.setPaused(true)
      expect(walletStore.getTxState().paused).toBe(true)
      expect(fires).toBe(1)
      // Idempotent: same value does not re-notify.
      walletStore.setPaused(true)
      expect(fires).toBe(1)
      unsubscribe()
    })

    it('approve writes per-token allowance and is idempotent for repeats', () => {
      let fires = 0
      const unsubscribe = walletStore.subscribe(() => {
        fires += 1
      })
      walletStore.approve('USDC', '500')
      expect(walletStore.getTxState().allowances).toEqual({ weth: '0', usdc: '500' })
      expect(fires).toBe(1)
      walletStore.approve('USDC', '500')
      expect(fires).toBe(1)
      walletStore.approve('WETH', '0.5')
      expect(walletStore.getTxState().allowances).toEqual({ weth: '0.5', usdc: '500' })
      expect(fires).toBe(2)
      unsubscribe()
    })

    it('approve rejects negative or malformed amounts', () => {
      expect(() => walletStore.approve('USDC', '-1')).toThrow(/non-negative/)
      expect(() => walletStore.approve('USDC', 'banana')).toThrow(/decimal string/)
    })

    it('deposit moves balance wallet→internal, consumes allowance, and notifies', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      let fires = 0
      const unsubscribe = walletStore.subscribe(() => {
        fires += 1
      })
      walletStore.deposit('USDC', '250')
      const state = walletStore.getState()
      expect(state.walletBalances.usdc).toBe('750')
      expect(state.internalBalances.usdc).toBe('250')
      expect(walletStore.getTxState().allowances.usdc).toBe('250')
      // Two notifications: one for the wallet state, one for the tx state.
      expect(fires).toBe(2)
      unsubscribe()
    })

    it('deposit decimal amounts compose correctly across multiple operations', () => {
      walletStore.connect()
      walletStore.approve('WETH', '1')
      walletStore.deposit('WETH', '0.25')
      walletStore.deposit('WETH', '0.5')
      const state = walletStore.getState()
      expect(state.walletBalances.weth).toBe('0.25')
      expect(state.internalBalances.weth).toBe('0.75')
      expect(walletStore.getTxState().allowances.weth).toBe('0.25')
    })

    it('deposit rejects when amount > wallet balance', () => {
      walletStore.connect()
      walletStore.approve('USDC', '99999')
      expect(() => walletStore.deposit('USDC', '1001')).toThrow(/insufficient wallet balance/)
    })

    it('deposit rejects when amount > allowance even if wallet is funded', () => {
      walletStore.connect()
      walletStore.approve('USDC', '100')
      expect(() => walletStore.deposit('USDC', '101')).toThrow(/insufficient allowance/)
    })

    it('deposit rejects when paused', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      walletStore.setPaused(true)
      expect(() => walletStore.deposit('USDC', '50')).toThrow(/paused/)
    })

    it('deposit rejects when amount is zero or negative', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      expect(() => walletStore.deposit('USDC', '0')).toThrow(/greater than zero/)
      expect(() => walletStore.deposit('USDC', '-1')).toThrow(/non-negative/)
    })

    it('deposit rejects when disconnected', () => {
      expect(() => walletStore.deposit('USDC', '1')).toThrow(/disconnected/)
    })

    it('withdraw moves balance internal→wallet and notifies once', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      walletStore.deposit('USDC', '500')
      let fires = 0
      const unsubscribe = walletStore.subscribe(() => {
        fires += 1
      })
      walletStore.withdraw('USDC', '200')
      const state = walletStore.getState()
      expect(state.walletBalances.usdc).toBe('700')
      expect(state.internalBalances.usdc).toBe('300')
      // Withdraw touches only the wallet slice.
      expect(fires).toBe(1)
      unsubscribe()
    })

    it('withdraw rejects when amount > internal balance', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      walletStore.deposit('USDC', '100')
      expect(() => walletStore.withdraw('USDC', '101')).toThrow(/insufficient DarkPool balance/)
    })

    it('withdraw rejects when paused', () => {
      walletStore.connect()
      walletStore.approve('USDC', '500')
      walletStore.deposit('USDC', '500')
      walletStore.setPaused(true)
      expect(() => walletStore.withdraw('USDC', '10')).toThrow(/paused/)
    })

    it('disconnect does not reset tx state (allowances persist)', () => {
      walletStore.connect()
      walletStore.approve('USDC', '123')
      walletStore.disconnect()
      expect(walletStore.getTxState().allowances.usdc).toBe('123')
    })

    it('resetTxState clears allowances and paused', () => {
      walletStore.connect()
      walletStore.approve('USDC', '50')
      walletStore.setPaused(true)
      walletStore.resetTxState()
      const tx = walletStore.getTxState()
      expect(tx).toEqual({ paused: false, allowances: { weth: '0', usdc: '0' } })
    })
  })
})
