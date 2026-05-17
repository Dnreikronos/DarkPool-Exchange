// `TokenSymbol` is derived from `TOKEN_DECIMALS` in lib/units so the
// pair stays in lockstep — adding a token there extends this type, and
// no second definition can drift away from the on-chain contract.
export type { TokenSymbol } from '../units'

export type Address = `0x${string}`

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
