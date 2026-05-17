'use client'

import { createContext, useContext, useMemo, type ReactNode } from 'react'

import { config } from '../config'
import { createDarkPoolClient, methodOverridesFromEnv, type DarkPoolClient } from './client'

const DarkPoolClientContext = createContext<DarkPoolClient | null>(null)

export interface DarkPoolClientProviderProps {
  /**
   * Optional pre-built client. When omitted, the provider lazily builds one
   * from `lib/config.ts` + per-method env overrides — the production path.
   * Tests and Storybook pass an explicit client.
   */
  client?: DarkPoolClient
  children: ReactNode
}

export function DarkPoolClientProvider({
  client,
  children,
}: DarkPoolClientProviderProps): JSX.Element {
  const value = useMemo(() => client ?? getDefaultClient(), [client])
  return <DarkPoolClientContext.Provider value={value}>{children}</DarkPoolClientContext.Provider>
}

export function useDarkPoolClient(): DarkPoolClient {
  const ctx = useContext(DarkPoolClientContext)
  if (!ctx) {
    throw new Error('useDarkPoolClient must be used inside <DarkPoolClientProvider>.')
  }
  return ctx
}

let cachedDefault: DarkPoolClient | null = null

/**
 * Lazily-instantiated default client backed by `lib/config.ts`. Lives at
 * module scope so navigating between trading routes keeps the same handle
 * (and any future in-flight state inside `MockClient`).
 */
function getDefaultClient(): DarkPoolClient {
  if (cachedDefault) return cachedDefault
  cachedDefault = createDarkPoolClient({
    baseUrl: config.apiUrl,
    apiKey: config.apiKey,
    useMocks: config.useMocks,
    methodOverrides: methodOverridesFromEnv(),
  })
  return cachedDefault
}
