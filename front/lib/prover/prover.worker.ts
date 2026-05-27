/// <reference lib="webworker" />

import init, { prove_order_wasm } from './zk-pkg/dp_zk_wasm'

export type ProverRequest =
  | { type: 'init' }
  | { type: 'prove'; id: string; witness: WitnessInput }

export interface WitnessInput {
  commitment_key: string
  side: number
  price: string
  size: string
  salt_hex: string
}

export type ProverResponse =
  | { type: 'ready' }
  | { type: 'progress'; stage: 'loading' | 'keygen' | 'proving'; pct: number }
  | { type: 'result'; id: string; proof: Uint8Array; vk: Uint8Array; commitment: Uint8Array }
  | { type: 'error'; id: string; message: string }

let wasmReady = false

async function loadWasm() {
  const wasmUrl = new URL('./zk-pkg/dp_zk_wasm_bg.wasm', import.meta.url)
  await init(wasmUrl)
  wasmReady = true
}

function parseProveResult(buf: Uint8Array): {
  proof: Uint8Array
  vk: Uint8Array
  commitment: Uint8Array
} {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength)
  const proofLen = view.getUint32(0, true)
  const vkLen = view.getUint32(4, true)
  const commitmentLen = view.getUint32(8, true)

  const proofStart = 12
  const vkStart = proofStart + proofLen
  const commitmentStart = vkStart + vkLen

  return {
    proof: buf.slice(proofStart, proofStart + proofLen),
    vk: buf.slice(vkStart, vkStart + vkLen),
    commitment: buf.slice(commitmentStart, commitmentStart + commitmentLen),
  }
}

const ctx = self as unknown as DedicatedWorkerGlobalScope

ctx.onmessage = async (e: MessageEvent<ProverRequest>) => {
  const msg = e.data

  if (msg.type === 'init') {
    try {
      ctx.postMessage({ type: 'progress', stage: 'loading', pct: 0 } satisfies ProverResponse)
      await loadWasm()
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
      if (!wasmReady) {
        ctx.postMessage({ type: 'progress', stage: 'loading', pct: 0 } satisfies ProverResponse)
        await loadWasm()
      }

      ctx.postMessage({ type: 'progress', stage: 'keygen', pct: 10 } satisfies ProverResponse)

      const witnessJson = JSON.stringify(msg.witness)

      ctx.postMessage({ type: 'progress', stage: 'proving', pct: 30 } satisfies ProverResponse)
      const resultBuf = prove_order_wasm(witnessJson)
      const { proof, vk, commitment } = parseProveResult(resultBuf)

      ctx.postMessage({
        type: 'result',
        id: msg.id,
        proof,
        vk,
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
