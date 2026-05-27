'use client'

import { useCallback, useEffect, useRef, useState } from 'react'

import type { ProverRequest, ProverResponse, WitnessInput } from './prover.worker'

export type ProverState = 'idle' | 'initializing' | 'proving' | 'ready' | 'error'

export interface ProverProgress {
  stage: 'loading' | 'keygen' | 'proving'
  pct: number
}

export interface ProveResult {
  proof: Uint8Array
  vk: Uint8Array
  commitment: Uint8Array
}

export interface UseProverReturn {
  state: ProverState
  progress: ProverProgress | null
  prove: (witness: WitnessInput) => Promise<ProveResult>
}

export function useProver(): UseProverReturn {
  const [state, setState] = useState<ProverState>('idle')
  const [progress, setProgress] = useState<ProverProgress | null>(null)
  const workerRef = useRef<Worker | null>(null)
  const pendingRef = useRef<Map<string, {
    resolve: (r: ProveResult) => void
    reject: (e: Error) => void
  }>>(new Map())
  const idCounter = useRef(0)

  const getWorker = useCallback((): Worker => {
    if (workerRef.current) return workerRef.current

    const w = new Worker(new URL('./prover.worker.ts', import.meta.url), {
      type: 'module',
    })

    w.onmessage = (e: MessageEvent<ProverResponse>) => {
      const msg = e.data

      if (msg.type === 'progress') {
        setProgress({ stage: msg.stage, pct: msg.pct })
        return
      }

      if (msg.type === 'ready') {
        setState('ready')
        setProgress(null)
        return
      }

      if (msg.type === 'result') {
        setState('ready')
        setProgress(null)
        const pending = pendingRef.current.get(msg.id)
        if (pending) {
          pendingRef.current.delete(msg.id)
          pending.resolve({
            proof: msg.proof,
            vk: msg.vk,
            commitment: msg.commitment,
          })
        }
        return
      }

      if (msg.type === 'error') {
        setState('error')
        setProgress(null)
        const pending = pendingRef.current.get(msg.id)
        if (pending) {
          pendingRef.current.delete(msg.id)
          pending.reject(new Error(msg.message))
        }
      }
    }

    workerRef.current = w
    return w
  }, [])

  const prove = useCallback(
    (witness: WitnessInput): Promise<ProveResult> => {
      return new Promise((resolve, reject) => {
        const id = String(++idCounter.current)
        const w = getWorker()
        pendingRef.current.set(id, { resolve, reject })

        setState('proving')
        setProgress({ stage: 'loading', pct: 0 })

        w.postMessage({ type: 'prove', id, witness } satisfies ProverRequest)
      })
    },
    [getWorker]
  )

  useEffect(() => {
    const worker = workerRef
    const pending = pendingRef
    return () => {
      worker.current?.terminate()
      worker.current = null
      for (const [, p] of pending.current) {
        p.reject(new Error('Worker terminated'))
      }
      pending.current.clear()
    }
  }, [])

  return { state, progress, prove }
}
