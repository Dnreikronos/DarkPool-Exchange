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

export interface Stage {
  kind: StageKind
  /** Filled only when `kind === 'error'`. */
  errorMessage?: string
}

export type StageAction =
  | { type: 'start'; needsApproval: boolean }
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
      return { kind: action.needsApproval ? 'approving' : 'submitting' }
    case 'approvalDone':
      if (state.kind !== 'approving') return state
      return { kind: 'submitting' }
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
