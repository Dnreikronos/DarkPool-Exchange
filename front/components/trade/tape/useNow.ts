'use client'

import { useEffect, useState } from 'react'

/**
 * Returns the current Unix time in seconds and updates once per second.
 * Single shared subscription pattern: every consumer mounts its own
 * interval, but the cost is one setState per second per consumer —
 * negligible for the tape (one parent component).
 *
 * Pass `nowSecondsOverride` (tests, Ladle stories) to freeze time.
 */
export function useNow(nowSecondsOverride?: number): number {
  const [now, setNow] = useState<number>(() =>
    nowSecondsOverride ?? Math.floor(Date.now() / 1000)
  )
  useEffect(() => {
    if (nowSecondsOverride !== undefined) return
    const id = setInterval(() => {
      setNow(Math.floor(Date.now() / 1000))
    }, 1000)
    return () => clearInterval(id)
  }, [nowSecondsOverride])
  return now
}
