'use client'

import { useCallback, useRef } from 'react'
import { create } from '@bufbuild/protobuf'

import { config } from '@/lib/config'
import { encryptOrder, serializeOrder, useOperatorPubkey } from '@/lib/crypto'
import { useProver } from '@/lib/prover'
import { PlaceOrderRequestSchema } from '@/lib/sdk'
import { useDarkPoolClient } from '@/lib/api-client'
import { useTraderId } from '@/lib/wallet/hooks'

import { createRealSteps, randomHex, type StageStep } from '../../_lib/entry/build-submission'
import { ORDER_PAIR, ORDER_TTL_NS } from '../../_lib/entry/policy'
import type { SubmitPayload } from './useSubmitStages'

export interface UseRealSubmissionResult {
  buildSteps: (payload: SubmitPayload) => StageStep[]
  /** Live proving percentage (0-100) for the progress bar, or null. */
  provingPct: number | null
}

export function useRealSubmission(): UseRealSubmissionResult {
  const trader = useTraderId()
  const { prove, progress } = useProver()
  const client = useDarkPoolClient()

  // The operator pubkey is fetched via TanStack Query; the steps read the
  // latest value through a ref so a slow fetch doesn't capture a stale
  // undefined. `enabled` is false in mock mode, so this is inert there.
  const pubkeyQuery = useOperatorPubkey(config.apiUrl, config.useMocks)
  const pubkeyRef = useRef<Uint8Array | undefined>(undefined)
  pubkeyRef.current = pubkeyQuery.data

  const buildSteps = useCallback(
    (payload: SubmitPayload): StageStep[] =>
      createRealSteps({
        trader: trader ?? '',
        pair: ORDER_PAIR,
        ttlNs: ORDER_TTL_NS,
        side: payload.side,
        price: payload.price,
        size: payload.size,
        randomHex,
        getOperatorPubkey: () => {
          const pk = pubkeyRef.current
          if (!pk) throw new Error('Operator key is still loading. Try again in a moment.')
          return pk
        },
        prove: (witness) => prove(witness),
        serialize: serializeOrder,
        encrypt: encryptOrder,
        placeOrder: (trio) => client.placeOrder(create(PlaceOrderRequestSchema, trio)),
      }),
    [trader, prove, client]
  )

  const provingPct = progress ? progress.pct : null

  return { buildSteps, provingPct }
}
