import type { Address, Balances, WalletState } from './types'

export const MOCK_ADDRESS: Address = '0x1111111111111111111111111111111111111111'

const ZERO_BALANCES: Balances = { weth: '0', usdc: '0' }
const INITIAL_WALLET_BALANCES: Balances = { weth: '1', usdc: '1000' }

const DISCONNECTED_STATE: WalletState = {
  status: 'disconnected',
  address: null,
  walletBalances: ZERO_BALANCES,
  internalBalances: ZERO_BALANCES,
}

type Listener = () => void

class MockWalletStore {
  private state: WalletState = DISCONNECTED_STATE
  private readonly listeners = new Set<Listener>()

  getState = (): WalletState => this.state

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  connect = (): void => {
    if (this.state.status === 'connected') return
    this.setState({
      status: 'connected',
      address: MOCK_ADDRESS,
      walletBalances: { ...INITIAL_WALLET_BALANCES },
      internalBalances: { ...ZERO_BALANCES },
    })
  }

  disconnect = (): void => {
    if (this.state.status === 'disconnected') return
    this.setState(DISCONNECTED_STATE)
  }

  private setState(next: WalletState): void {
    this.state = next
    this.listeners.forEach((listener) => listener())
  }
}

export const walletStore = new MockWalletStore()
