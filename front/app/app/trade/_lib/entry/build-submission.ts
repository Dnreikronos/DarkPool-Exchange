// Pure builders for the real submission pipeline (#99). No React, no
// network, no worker — everything is injected so this is node-testable.
// The order's blinding nonce is `commitment_key`: ONE random hex value is
// generated per submission and threaded into BOTH the prover witness and
// the encrypted order payload. The operator recomputes the canonical
// Poseidon commitment from the decrypted payload, so the client never
// sends a salt (dp-engine/src/engine.rs:487-490).

import type { DecryptedOrderPayload } from '@/lib/crypto'
import type { WitnessInput } from '@/lib/prover'

import type { OrderSide } from './validate'

/** A single stage of the submission state machine. */
export interface StageStep {
  id: 'preparing' | 'proving' | 'encrypting' | 'submitting'
  run: (ctx: { aborted: () => boolean }) => Promise<void>
}

function sideToNum(side: OrderSide): 0 | 1 {
  return side === 'buy' ? 0 : 1
}

/** Cryptographically-random lowercase hex string of `nBytes` bytes. */
export function randomHex(nBytes: number): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(nBytes)), (b) =>
    b.toString(16).padStart(2, '0')
  ).join('')
}

export function buildWitness(args: {
  commitmentKey: string
  saltHex: string
  side: OrderSide
  price: string
  size: string
}): WitnessInput {
  return {
    commitment_key: args.commitmentKey,
    side: sideToNum(args.side),
    price: args.price,
    size: args.size,
    salt_hex: args.saltHex,
  }
}

export function buildOrderPayload(args: {
  trader: string
  pair: string
  side: OrderSide
  price: string
  size: string
  commitmentKey: string
  ttlNs: number
}): DecryptedOrderPayload {
  return {
    trader: args.trader,
    pair: args.pair,
    side: sideToNum(args.side),
    price: args.price,
    size: args.size,
    commitment_key: args.commitmentKey,
    ttl: args.ttlNs,
  }
}

export interface RealStepDeps {
  trader: string
  pair: string
  ttlNs: number
  side: OrderSide
  price: string
  size: string
  randomHex: (nBytes: number) => string
  /** Latest resolved operator SEC1 pubkey bytes; throws if not loaded yet. */
  getOperatorPubkey: () => Uint8Array
  prove: (witness: WitnessInput) => Promise<{ proof: Uint8Array; commitment: Uint8Array }>
  serialize: (payload: DecryptedOrderPayload) => Uint8Array
  encrypt: (bytes: Uint8Array, pubkey: Uint8Array) => Uint8Array
  placeOrder: (req: {
    commitment: Uint8Array
    proof: Uint8Array
    encryptedPayload: Uint8Array
  }) => Promise<unknown>
}

/**
 * Build the four real stage steps. The steps share a private draft so the
 * commitment_key minted in `preparing` reaches `proving`/`encrypting`, and
 * the prove output reaches `submitting`.
 */
export function createRealSteps(deps: RealStepDeps): StageStep[] {
  const draft: {
    witness?: WitnessInput
    payload?: DecryptedOrderPayload
    proof?: Uint8Array
    commitment?: Uint8Array
    encryptedPayload?: Uint8Array
  } = {}

  return [
    {
      id: 'preparing',
      run: async () => {
        if (!deps.trader) throw new Error('Connect a wallet to place orders.')
        const commitmentKey = deps.randomHex(32)
        const saltHex = deps.randomHex(32)
        draft.witness = buildWitness({
          commitmentKey,
          saltHex,
          side: deps.side,
          price: deps.price,
          size: deps.size,
        })
        draft.payload = buildOrderPayload({
          trader: deps.trader,
          pair: deps.pair,
          side: deps.side,
          price: deps.price,
          size: deps.size,
          commitmentKey,
          ttlNs: deps.ttlNs,
        })
      },
    },
    {
      id: 'proving',
      run: async () => {
        const { proof, commitment } = await deps.prove(draft.witness!)
        draft.proof = proof
        draft.commitment = commitment
      },
    },
    {
      id: 'encrypting',
      run: async () => {
        const bytes = deps.serialize(draft.payload!)
        draft.encryptedPayload = deps.encrypt(bytes, deps.getOperatorPubkey())
      },
    },
    {
      id: 'submitting',
      run: async () => {
        await deps.placeOrder({
          commitment: draft.commitment!,
          proof: draft.proof!,
          encryptedPayload: draft.encryptedPayload!,
        })
      },
    },
  ]
}
