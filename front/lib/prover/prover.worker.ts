/// <reference lib="webworker" />

import init, { prove_order_wasm } from './zk-pkg/dp_zk_wasm'

export type ProverRequest = { type: 'init' } | { type: 'prove'; id: string; witness: WitnessInput }

export interface WitnessInput {
  trader_addr: string
  side: number
  price: string
  size: string
  salt_hex: string
}

export type ProverResponse =
  | { type: 'ready' }
  | { type: 'progress'; stage: 'loading' | 'proving'; pct: number }
  | { type: 'result'; id: string; proof: Uint8Array; commitment: Uint8Array }
  | { type: 'error'; id: string; message: string }

// Canonical proving key from the trusted-setup ceremony, served as a static
// asset. The prover proves against this pinned key and never mints its own
// verifying key (issue #212). Swapped for the real ceremony key in #97/#98.
const PROVING_KEY_URL = '/zk/commitment_pk.bin'

let wasmReady = false
let wasmInitPromise: Promise<void> | null = null

let provingKey: Uint8Array | null = null
let provingKeyPromise: Promise<Uint8Array> | null = null

async function loadWasm() {
  if (wasmReady) return
  if (!wasmInitPromise) {
    const wasmUrl = new URL('./zk-pkg/dp_zk_wasm_bg.wasm', import.meta.url)
    wasmInitPromise = init(wasmUrl)
      .then(() => {
        wasmReady = true
      })
      .finally(() => {
        if (!wasmReady) wasmInitPromise = null
      })
  }
  await wasmInitPromise
}

async function loadProvingKey(): Promise<Uint8Array> {
  if (provingKey) return provingKey
  if (!provingKeyPromise) {
    provingKeyPromise = fetch(new URL(PROVING_KEY_URL, self.location.origin))
      .then((res) => {
        if (!res.ok) throw new Error(`proving key fetch failed: HTTP ${res.status}`)
        return res.arrayBuffer()
      })
      .then((buf) => {
        provingKey = new Uint8Array(buf)
        return provingKey
      })
      .finally(() => {
        if (!provingKey) provingKeyPromise = null
      })
  }
  return provingKeyPromise
}

function parseProveResult(buf: Uint8Array): {
  proof: Uint8Array
  commitment: Uint8Array
} {
  if (buf.byteLength < 8) {
    throw new Error('Invalid prove result: header too short')
  }

  // Wire format: [proof_len(4) | commitment_len(4) | proof | commitment].
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength)
  const proofLen = view.getUint32(0, true)
  const commitmentLen = view.getUint32(4, true)

  const proofStart = 8
  const commitmentStart = proofStart + proofLen
  const end = commitmentStart + commitmentLen

  if (end > buf.byteLength) {
    throw new Error('Invalid prove result: truncated payload')
  }

  return {
    proof: buf.slice(proofStart, proofStart + proofLen),
    commitment: buf.slice(commitmentStart, commitmentStart + commitmentLen),
  }
}

const ctx = self as unknown as DedicatedWorkerGlobalScope

ctx.onmessage = async (e: MessageEvent<ProverRequest>) => {
  const msg = e.data

  if (msg.type === 'init') {
    try {
      ctx.postMessage({ type: 'progress', stage: 'loading', pct: 0 } satisfies ProverResponse)
      await Promise.all([loadWasm(), loadProvingKey()])
      ctx.postMessage({ type: 'ready' } satisfies ProverResponse)
    } catch (err) {
      ctx.postMessage({
        type: 'error',
        id: '',
        message: err instanceof Error ? err.message : String(err),
      } satisfies ProverResponse)
    }
    return
  }

  if (msg.type === 'prove') {
    try {
      ctx.postMessage({ type: 'progress', stage: 'loading', pct: 0 } satisfies ProverResponse)
      const [, pk] = await Promise.all([loadWasm(), loadProvingKey()])

      const witnessJson = JSON.stringify(msg.witness)

      ctx.postMessage({ type: 'progress', stage: 'proving', pct: 30 } satisfies ProverResponse)
      const resultBuf = prove_order_wasm(witnessJson, pk)
      const { proof, commitment } = parseProveResult(resultBuf)

      ctx.postMessage({
        type: 'result',
        id: msg.id,
        proof,
        commitment,
      } satisfies ProverResponse)
    } catch (err) {
      ctx.postMessage({
        type: 'error',
        id: msg.id,
        message: err instanceof Error ? err.message : String(err),
      } satisfies ProverResponse)
    }
  }
}
