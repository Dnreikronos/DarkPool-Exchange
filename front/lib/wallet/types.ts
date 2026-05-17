export type Address = `0x${string}`

export type TokenSymbol = 'WETH' | 'USDC'

export type WalletStatus = 'disconnected' | 'connecting' | 'connected'

export interface Balances {
  weth: string
  usdc: string
}

export interface WalletState {
  status: WalletStatus
  address: Address | null
  walletBalances: Balances
  internalBalances: Balances
}
