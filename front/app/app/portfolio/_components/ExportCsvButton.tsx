'use client'

import * as React from 'react'

import type { Fill } from '@/lib/mock-store'

import { fillsToCsv } from './csv'

export interface ExportCsvButtonProps {
  fills: readonly Fill[]
}

/**
 * Triggers a client-side download of the current fill history. Stays
 * disabled when there is nothing to export so the button can never
 * dispatch a stub CSV that misleads a user post-disconnect.
 *
 * SSR-safe: the URL.createObjectURL path only runs inside the click
 * handler, after hydration. The component itself does not touch
 * `window` during render.
 */
export function ExportCsvButton({ fills }: ExportCsvButtonProps): JSX.Element {
  const disabled = fills.length === 0
  const handleClick = React.useCallback(() => {
    if (typeof window === 'undefined') return
    const csv = fillsToCsv(fills)
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filenameFor(new Date())
    document.body.appendChild(anchor)
    anchor.click()
    document.body.removeChild(anchor)
    URL.revokeObjectURL(url)
  }, [fills])

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={disabled}
      className="border border-brand-border px-3 py-1 font-mono text-label-md uppercase tracking-labelWide text-brand-muted transition-colors hover:border-brand-muted hover:text-brand-fg disabled:cursor-not-allowed disabled:opacity-50 focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent"
      aria-label="Export fill history as CSV"
    >
      [ EXPORT CSV ]
    </button>
  )
}

function filenameFor(d: Date): string {
  const pad = (n: number) => (n < 10 ? `0${n}` : `${n}`)
  return `darkpool-fills-${d.getUTCFullYear()}${pad(d.getUTCMonth() + 1)}${pad(d.getUTCDate())}.csv`
}
