// Ladle/Vite-only side-effect: lib/config.ts reads process.env at module
// load and the browser bundle has no `process` global. This shim fills it
// with sensible mock defaults so `parseConfig()` succeeds with `useMocks=true`.
// Production Next.js builds inline NEXT_PUBLIC_* at compile time and never
// hit this branch.

const g = globalThis as unknown as { process?: { env: Record<string, string | undefined> } }

if (typeof g.process === 'undefined') {
  g.process = {
    env: {
      NEXT_PUBLIC_USE_MOCKS: 'true',
      NEXT_PUBLIC_DARKPOOL_API_URL: 'http://localhost:8080',
      NEXT_PUBLIC_DARKPOOL_API_KEY: 'ladle-dev',
      NEXT_PUBLIC_CHAIN_ID: '42161',
      NEXT_PUBLIC_OPERATOR_PUBKEY_URL: 'http://localhost:8080/v1/operator/pubkey',
    },
  }
}

export {}
