import { useQuery } from '@tanstack/react-query'
import { hexToBytes } from './hex'

const STALE_TIME = 5 * 60 * 1000

interface PubkeyResponse {
  pubkey: string
  encoding: string
}

async function fetchOperatorPubkey(baseUrl: string): Promise<Uint8Array> {
  const res = await fetch(`${baseUrl}/v1/operator/pubkey`)
  if (!res.ok) {
    throw new Error(`Failed to fetch operator pubkey: ${res.status}`)
  }
  const body: PubkeyResponse = await res.json()
  return hexToBytes(body.pubkey)
}

export function useOperatorPubkey(baseUrl: string, useMocks = false) {
  return useQuery({
    queryKey: ['operator-pubkey', baseUrl],
    queryFn: () => fetchOperatorPubkey(baseUrl),
    staleTime: STALE_TIME,
    enabled: !useMocks,
  })
}
