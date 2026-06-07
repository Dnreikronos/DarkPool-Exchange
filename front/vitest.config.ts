import { defineConfig } from 'vitest/config'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  esbuild: {
    // Use the new automatic JSX transform so files can omit `import React`
    // (matches Next.js / SWC behaviour). Without this, provider.tsx and other
    // library files that rely on the automatic runtime crash under Vitest.
    jsx: 'automatic',
    jsxImportSource: 'react',
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('.', import.meta.url)),
    },
  },
  test: {
    env: {
      NEXT_PUBLIC_USE_MOCKS: 'true',
      NEXT_PUBLIC_DARKPOOL_API_URL: 'http://localhost:8080',
      NEXT_PUBLIC_DARKPOOL_API_KEY: 'test-key',
      NEXT_PUBLIC_CHAIN_ID: '31337',
      NEXT_PUBLIC_OPERATOR_PUBKEY_URL: 'http://localhost:8080/v1/operator/pubkey',
    },
  },
})
