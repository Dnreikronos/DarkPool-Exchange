// Pure reducer for the deposit/withdraw transaction staging.
//
// Splitting the stage transitions out as a reducer keeps the hook layer
// thin (it only owns the 1s timers + the side-effecting store calls)
// and means the contract is exhaustively unit-testable in node with no
// React or DOM.
//
// Deposit flow:  idle -> [approving]? -> submitting -> confirmed -> close
// Withdraw flow: idle ->                 submitting -> confirmed -> close
// Error path:    any in-flight stage -> error -> reset returns to idle.

export type StageKind = 'idle' | 'approving' | 'submitting' | 'confirmed' | 'error'

/**
 * In-flight sub-phase for the `approving` / `submitting` stages, mapping
 * to wagmi's write lifecycle: `signing` is `useWriteContract`'s
 * `isPending` (the wallet prompt is open), `mining` is
 * `useWaitForTransactionReceipt`'s `isLoading` (the tx is on-chain,
 * awaiting confirmation). Undefined for non-in-flight kinds.
 */
export type StagePhase = 'signing' | 'mining'

export interface Stage {
  kind: StageKind
  /** Filled only when `kind` is `approving` | `submitting`. */
  phase?: StagePhase
  /** Filled only when `kind === 'error'`. */
  errorMessage?: string
}

export type StageAction =
  | { type: 'start'; needsApproval: boolean }
  | { type: 'signed' }
  | { type: 'approvalDone' }
  | { type: 'submitted' }
  | { type: 'fail'; message: string }
  | { type: 'reset' }

export const INITIAL_STAGE: Stage = { kind: 'idle' }

export function reduceStage(state: Stage, action: StageAction): Stage {
  switch (action.type) {
    case 'start':
      // start is a no-op while a tx is in flight; the user must reset
      // (e.g. close the modal) before kicking off a new one. Allowed
      // entry points: idle, or recovering from an error.
      if (state.kind !== 'idle' && state.kind !== 'error') return state
      return { kind: action.needsApproval ? 'approving' : 'submitting', phase: 'signing' }
    case 'signed':
      // Hash received: the open wallet prompt closed and the tx is now
      // mining. Only advances a signing in-flight stage; mining/idle/
      // terminal states ignore it.
      if (state.kind !== 'approving' && state.kind !== 'submitting') return state
      if (state.phase !== 'signing') return state
      return { kind: state.kind, phase: 'mining' }
    case 'approvalDone':
      if (state.kind !== 'approving') return state
      return { kind: 'submitting', phase: 'signing' }
    case 'submitted':
      if (state.kind !== 'submitting') return state
      return { kind: 'confirmed' }
    case 'fail':
      // A fail is meaningful only while a tx is mid-flight; idle/
      // confirmed/error states ignore it so a stale timer callback
      // can't undo a happy-path completion.
      if (state.kind !== 'approving' && state.kind !== 'submitting') return state
      return { kind: 'error', errorMessage: action.message }
    case 'reset':
      return INITIAL_STAGE
  }
}

export function isInFlight(stage: Stage): boolean {
  return stage.kind === 'approving' || stage.kind === 'submitting'
}

export function isTerminal(stage: Stage): boolean {
  return stage.kind === 'confirmed' || stage.kind === 'error'
}
