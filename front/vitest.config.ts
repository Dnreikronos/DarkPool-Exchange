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
})
