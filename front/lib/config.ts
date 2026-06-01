import { z } from 'zod'

const addressSchema = z
  .string()
  .regex(/^0x[a-fA-F0-9]{40}$/, 'must be a 0x-prefixed 20-byte hex address')

const booleanFromEnv = z
  .enum(['true', 'false', '1', '0'])
  .transform((value) => value === 'true' || value === '1')

// Like booleanFromEnv but optional: an unset var resolves to `false` rather
// than throwing, so existing .env.local files keep booting when a new opt-in
// flag is added.
const optionalBooleanFromEnv = z
  .enum(['true', 'false', '1', '0'])
  .optional()
  .transform((value) => value === 'true' || value === '1')

const chainIdSchema = z
  .string()
  .regex(/^\d+$/, 'must be a positive integer')
  .transform((value) => Number.parseInt(value, 10))
  .pipe(z.number().int().positive())

const urlSchema = z.string().url()

const baseSchema = z.object({
  NEXT_PUBLIC_USE_MOCKS: booleanFromEnv,
  NEXT_PUBLIC_DARKPOOL_API_URL: urlSchema,
  NEXT_PUBLIC_DARKPOOL_API_KEY: z.string().min(1),
  NEXT_PUBLIC_CHAIN_ID: chainIdSchema,
  NEXT_PUBLIC_OPERATOR_PUBKEY_URL: urlSchema,
  // Opt-in per-user SIWE auth. Defaults false so mocks and non-SIWE
  // deployments keep using the static x-api-key.
  NEXT_PUBLIC_SIWE_ENABLED: optionalBooleanFromEnv,
})

const contractsSchema = z.object({
  NEXT_PUBLIC_DARKPOOL_ADDRESS: addressSchema,
  NEXT_PUBLIC_VERIFIER_PROXY_ADDRESS: addressSchema,
  NEXT_PUBLIC_USDC_ADDRESS: addressSchema,
  NEXT_PUBLIC_WETH_ADDRESS: addressSchema,
})

const rawEnv = {
  NEXT_PUBLIC_USE_MOCKS: process.env.NEXT_PUBLIC_USE_MOCKS,
  NEXT_PUBLIC_DARKPOOL_API_URL: process.env.NEXT_PUBLIC_DARKPOOL_API_URL,
  NEXT_PUBLIC_DARKPOOL_API_KEY: process.env.NEXT_PUBLIC_DARKPOOL_API_KEY,
  NEXT_PUBLIC_CHAIN_ID: process.env.NEXT_PUBLIC_CHAIN_ID,
  NEXT_PUBLIC_OPERATOR_PUBKEY_URL: process.env.NEXT_PUBLIC_OPERATOR_PUBKEY_URL,
  NEXT_PUBLIC_SIWE_ENABLED: process.env.NEXT_PUBLIC_SIWE_ENABLED,
  NEXT_PUBLIC_DARKPOOL_ADDRESS: process.env.NEXT_PUBLIC_DARKPOOL_ADDRESS,
  NEXT_PUBLIC_VERIFIER_PROXY_ADDRESS: process.env.NEXT_PUBLIC_VERIFIER_PROXY_ADDRESS,
  NEXT_PUBLIC_USDC_ADDRESS: process.env.NEXT_PUBLIC_USDC_ADDRESS,
  NEXT_PUBLIC_WETH_ADDRESS: process.env.NEXT_PUBLIC_WETH_ADDRESS,
}

type ContractAddresses = {
  darkPool: `0x${string}`
  verifierProxy: `0x${string}`
  usdc: `0x${string}`
  weth: `0x${string}`
}

type MockConfig = {
  useMocks: true
  apiUrl: string
  apiKey: string
  chainId: number
  operatorPubkeyUrl: string
  siweEnabled: boolean
  contracts: null
}

type LiveConfig = {
  useMocks: false
  apiUrl: string
  apiKey: string
  chainId: number
  operatorPubkeyUrl: string
  siweEnabled: boolean
  contracts: ContractAddresses
}

export type Config = MockConfig | LiveConfig

function formatIssues(error: z.ZodError): string {
  const lines = error.issues.map((issue) => {
    const path = issue.path.join('.') || '(root)'
    return `  • ${path}: ${issue.message}`
  })
  return [
    'Invalid trading-app environment variables:',
    ...lines,
    '',
    'See front/.env.local.example for the required shape and copy it to front/.env.local.',
  ].join('\n')
}

function parseConfig(): Config {
  const base = baseSchema.safeParse(rawEnv)
  if (!base.success) {
    throw new Error(formatIssues(base.error))
  }

  if (base.data.NEXT_PUBLIC_USE_MOCKS) {
    return {
      useMocks: true,
      apiUrl: base.data.NEXT_PUBLIC_DARKPOOL_API_URL,
      apiKey: base.data.NEXT_PUBLIC_DARKPOOL_API_KEY,
      chainId: base.data.NEXT_PUBLIC_CHAIN_ID,
      operatorPubkeyUrl: base.data.NEXT_PUBLIC_OPERATOR_PUBKEY_URL,
      siweEnabled: base.data.NEXT_PUBLIC_SIWE_ENABLED,
      contracts: null,
    }
  }

  const contracts = contractsSchema.safeParse(rawEnv)
  if (!contracts.success) {
    throw new Error(formatIssues(contracts.error))
  }

  return {
    useMocks: false,
    apiUrl: base.data.NEXT_PUBLIC_DARKPOOL_API_URL,
    apiKey: base.data.NEXT_PUBLIC_DARKPOOL_API_KEY,
    chainId: base.data.NEXT_PUBLIC_CHAIN_ID,
    operatorPubkeyUrl: base.data.NEXT_PUBLIC_OPERATOR_PUBKEY_URL,
    siweEnabled: base.data.NEXT_PUBLIC_SIWE_ENABLED,
    contracts: {
      darkPool: contracts.data.NEXT_PUBLIC_DARKPOOL_ADDRESS as `0x${string}`,
      verifierProxy: contracts.data.NEXT_PUBLIC_VERIFIER_PROXY_ADDRESS as `0x${string}`,
      usdc: contracts.data.NEXT_PUBLIC_USDC_ADDRESS as `0x${string}`,
      weth: contracts.data.NEXT_PUBLIC_WETH_ADDRESS as `0x${string}`,
    },
  }
}

export const config: Config = parseConfig()
