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

const WALLETCONNECT_PLACEHOLDER_ID = 'YOUR_PROJECT_ID'

// Chains where we treat the missing/placeholder projectId as a dev
// convenience. Everything else is considered production by default —
// if the supported-chains list ever grows a new L2, the safe-by-
// default behavior is to require the projectId until it's explicitly
// added here.
const DEV_CHAIN_IDS: ReadonlySet<number> = new Set<number>([
  sepolia.id,
  arbitrumSepolia.id,
  baseSepolia.id,
  foundry.id,
  hardhat.id,
])

/**
 * Resolves the WalletConnect project id from the environment.
 *
 * WalletConnect needs a real id (free, https://cloud.reown.com) for
 * the connector to function. On dev/testnet chains we accept the
 * placeholder so local workflows aren't blocked — RainbowKit logs a
 * warning and the WalletConnect option simply doesn't work; injected
 * wallets (MetaMask, Rabby…) keep working. On a production chain we
 * fail-fast at boot so a misconfigured prod build can't ship.
 */
export function resolveWalletConnectProjectId(chain: Chain): string {
  const raw = process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID ?? WALLETCONNECT_PLACEHOLDER_ID
  if (raw !== WALLETCONNECT_PLACEHOLDER_ID && raw.length > 0) {
    return raw
  }
  if (!DEV_CHAIN_IDS.has(chain.id)) {
    throw new Error(
      `NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID is required when running ` +
        `against ${chain.name} (chain ${chain.id}). Get a free id at ` +
        `https://cloud.reown.com and set it in front/.env.local.`
    )
  }
  return WALLETCONNECT_PLACEHOLDER_ID
}

let cachedConfig: ReturnType<typeof getDefaultConfig> | null = null

export function getWagmiConfig(): ReturnType<typeof getDefaultConfig> {
  if (cachedConfig) return cachedConfig
  cachedConfig = getDefaultConfig({
    appName: 'DarkPool',
    projectId: resolveWalletConnectProjectId(targetChain),
    chains: [targetChain],
    ssr: true,
  })
  return cachedConfig
}
