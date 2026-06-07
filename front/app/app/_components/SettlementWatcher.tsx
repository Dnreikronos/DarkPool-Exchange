'use client'

import { useSettlementWatch } from '@/lib/settlement'

/**
 * Headless mount point for the BatchSettled subscription (#100). Lives
 * in the /app layout so settlement events observed anywhere under
 * /app/* feed the session-wide settlement store regardless of which
 * panel is on screen.
 */
export function SettlementWatcher(): null {
  useSettlementWatch()
  return null
}
