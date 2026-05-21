import { getDefaultConfig } from '@rainbow-me/rainbowkit'
import {
  arbitrum,
  arbitrumSepolia,
  base,
  baseSepolia,
  foundry,
  hardhat,
  mainnet,
  sepolia,
  type Chain,
} from 'wagmi/chains'

import { config as appConfig } from '../config'

const SUPPORTED_CHAINS: readonly Chain[] = [
  mainnet,
  sepolia,
  arbitrum,
  arbitrumSepolia,
  base,
  baseSepolia,
  foundry,
  hardhat,
] as const

export function resolveTargetChain(chainId: number): Chain {
  const match = SUPPORTED_CHAINS.find((chain) => chain.id === chainId)
  if (!match) {
    throw new Error(
      `Unsupported NEXT_PUBLIC_CHAIN_ID=${chainId}. ` +
        `Known ids: ${SUPPORTED_CHAINS.map((c) => `${c.id} (${c.name})`).join(', ')}.`
    )
  }
  return match
}

export const targetChain: Chain = resolveTargetChain(appConfig.chainId)

// WalletConnect requires a project id (free, https://cloud.reown.com). When
// unset we still build a working config — RainbowKit logs a warning and
// the WalletConnect option is disabled, but injected (MetaMask, Rabby…)
// wallets keep working. Required in production.
const walletConnectProjectId = process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID ?? 'YOUR_PROJECT_ID'

let cachedConfig: ReturnType<typeof getDefaultConfig> | null = null

export function getWagmiConfig(): ReturnType<typeof getDefaultConfig> {
  if (cachedConfig) return cachedConfig
  cachedConfig = getDefaultConfig({
    appName: 'DarkPool',
    projectId: walletConnectProjectId,
    chains: [targetChain],
    ssr: true,
  })
  return cachedConfig
}
