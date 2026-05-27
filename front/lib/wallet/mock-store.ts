import { Decimal } from '../units'
import type { Address, Balances, TokenSymbol, WalletState } from './types'

export const MOCK_ADDRESS: Address = '0x1111111111111111111111111111111111111111'

const ZERO_BALANCES: Balances = { weth: '0', usdc: '0' }
const INITIAL_WALLET_BALANCES: Balances = { weth: '1', usdc: '1000' }

const DISCONNECTED_STATE: WalletState = {
  status: 'disconnected',
  address: null,
  walletBalances: ZERO_BALANCES,
  internalBalances: ZERO_BALANCES,
}

// Tx state lives alongside (not inside) WalletState so the existing
// reactive contract (balances + status) stays a one-purpose surface.
// Both slices share the same listener set: changes to tx state never
// re-render balance subscribers, because useSyncExternalStore compares
// the snapshot reference per slice. F1.5 (#72) is the only writer.
export interface TxState {
  paused: boolean
  allowances: Balances
}

const INITIAL_TX_STATE: TxState = {
  paused: false,
  allowances: ZERO_BALANCES,
}

type Listener = () => void

function keyFor(token: TokenSymbol): keyof Balances {
  return token === 'WETH' ? 'weth' : 'usdc'
}

function assertNonNegative(amount: string, label: string): Decimal {
  let d: Decimal
  try {
    d = new Decimal(amount)
  } catch {
    throw new RangeError(`walletStore: ${label} must be a decimal string (got "${amount}")`)
  }
  if (!d.isFinite() || d.isNegative()) {
    throw new RangeError(
      `walletStore: ${label} must be a non-negative finite decimal (got "${amount}")`
    )
  }
  return d
}

function add(a: string, b: Decimal): string {
  return new Decimal(a).plus(b).toFixed()
}

function sub(a: string, b: Decimal): string {
  return new Decimal(a).minus(b).toFixed()
}

class MockWalletStore {
  private state: WalletState = DISCONNECTED_STATE
  private tx: TxState = INITIAL_TX_STATE
  private readonly listeners = new Set<Listener>()

  getState = (): WalletState => this.state

  getTxState = (): TxState => this.tx

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  // `address` defaults to `MOCK_ADDRESS` so existing callers
  // (`walletStore.connect()` with no args) keep their byte-identical
  // commitment-key projection. The real-wallet bridge passes the
  // wagmi-derived address explicitly.
  connect = (address: Address = MOCK_ADDRESS): void => {
    if (this.state.status === 'connected' && this.state.address === address) return
    this.setWalletState({
      status: 'connected',
      address,
      walletBalances: { ...INITIAL_WALLET_BALANCES },
      internalBalances: { ...ZERO_BALANCES },
    })
  }

  disconnect = (): void => {
    if (this.state.status === 'disconnected') return
    this.setWalletState(DISCONNECTED_STATE)
  }

  setPaused = (paused: boolean): void => {
    if (this.tx.paused === paused) return
    this.setTxState({ ...this.tx, paused })
  }

  approve = (token: TokenSymbol, amount: string): void => {
    assertNonNegative(amount, 'allowance')
    const key = keyFor(token)
    if (this.tx.allowances[key] === amount) return
    this.setTxState({
      ...this.tx,
      allowances: { ...this.tx.allowances, [key]: amount },
    })
  }

  deposit = (token: TokenSymbol, amount: string): void => {
    if (this.state.status !== 'connected') {
      throw new Error('walletStore: cannot deposit while disconnected')
    }
    if (this.tx.paused) {
      throw new Error('walletStore: contract is paused')
    }
    const d = assertNonNegative(amount, 'deposit amount')
    if (d.isZero()) {
      throw new RangeError('walletStore: deposit amount must be greater than zero')
    }
    const key = keyFor(token)
    const wallet = new Decimal(this.state.walletBalances[key])
    if (d.gt(wallet)) {
      throw new RangeError(`walletStore: insufficient wallet balance for ${token}`)
    }
    const allowance = new Decimal(this.tx.allowances[key])
    if (d.gt(allowance)) {
      throw new RangeError(`walletStore: insufficient allowance for ${token}`)
    }
    this.setWalletState({
      ...this.state,
      walletBalances: {
        ...this.state.walletBalances,
        [key]: sub(this.state.walletBalances[key], d),
      },
      internalBalances: {
        ...this.state.internalBalances,
        [key]: add(this.state.internalBalances[key], d),
      },
    })
    // ERC-20 semantics: spent allowance is consumed.
    this.setTxState({
      ...this.tx,
      allowances: { ...this.tx.allowances, [key]: sub(this.tx.allowances[key], d) },
    })
  }

  withdraw = (token: TokenSymbol, amount: string): void => {
    if (this.state.status !== 'connected') {
      throw new Error('walletStore: cannot withdraw while disconnected')
    }
    if (this.tx.paused) {
      throw new Error('walletStore: contract is paused')
    }
    const d = assertNonNegative(amount, 'withdraw amount')
    if (d.isZero()) {
      throw new RangeError('walletStore: withdraw amount must be greater than zero')
    }
    const key = keyFor(token)
    const internal = new Decimal(this.state.internalBalances[key])
    if (d.gt(internal)) {
      throw new RangeError(`walletStore: insufficient DarkPool balance for ${token}`)
    }
    this.setWalletState({
      ...this.state,
      walletBalances: {
        ...this.state.walletBalances,
        [key]: add(this.state.walletBalances[key], d),
      },
      internalBalances: {
        ...this.state.internalBalances,
        [key]: sub(this.state.internalBalances[key], d),
      },
    })
  }

  resetTxState = (): void => {
    if (this.tx === INITIAL_TX_STATE) return
    this.setTxState(INITIAL_TX_STATE)
  }

  private setWalletState(next: WalletState): void {
    this.state = next
    this.notify()
  }

  private setTxState(next: TxState): void {
    this.tx = next
    this.notify()
  }

  private notify(): void {
    this.listeners.forEach((listener) => listener())
  }
}

export const walletStore = new MockWalletStore()
