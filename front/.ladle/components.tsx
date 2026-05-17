import * as React from 'react'
import type { GlobalProvider } from '@ladle/react'

import '../app/globals.css'

export const Provider: GlobalProvider = ({ children }) => (
  <div
    className="min-h-screen bg-brand-bg p-8 font-mono text-brand-fg"
    style={{ colorScheme: 'dark' }}
  >
    {children}
  </div>
)
