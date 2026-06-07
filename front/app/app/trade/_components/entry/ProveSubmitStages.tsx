'use client'

// Visual layer for the multi-stage submission. Renders the place-button
// label (which mutates through the stages, now with real elapsed seconds),
// the thin progress bar underneath it, and the inline error area below.
// Presentational only — state comes from useSubmitStages.

import * as React from 'react'

import { cn } from '@/components/ui/cn'

import { STAGE_LABELS, type SubmitStageId } from '../../_lib/entry/policy'
import type { SubmissionPhase } from '../../_hooks/entry/useSubmitStages'

export interface PlaceButtonProps {
  /** What the button reads when idle (e.g. "BUY · WETH"). */
  idleLabel: string
  phase: SubmissionPhase
  disabled?: boolean
  onClick: () => void
  /** Lime accent surface when the form is valid and idle. */
  accent: boolean
  /** Live proving percentage (0-100), used for the proving-stage bar. */
  provingPct?: number | null
  /** Injectable clock for tests; defaults to Date.now. */
  now?: () => number
}

/** Live seconds since the current running stage started (0 when unknown). */
function useStageElapsed(phase: SubmissionPhase, now: () => number): number {
  const [, force] = React.useState(0)
  const running = phase.kind === 'running' && phase.stageStartedAtMs !== undefined
  const stageKey = phase.kind === 'running' ? phase.stage : null
  React.useEffect(() => {
    if (!running) return
    const id = setInterval(() => force((n) => n + 1), 100)
    return () => clearInterval(id)
  }, [running, stageKey])

  if (phase.kind !== 'running' || phase.stageStartedAtMs === undefined) return 0
  return Math.max(0, (now() - phase.stageStartedAtMs) / 1000)
}

export function PlaceButton({
  idleLabel,
  phase,
  disabled,
  onClick,
  accent,
  provingPct,
  now = Date.now,
}: PlaceButtonProps) {
  const elapsed = useStageElapsed(phase, now)
  const label = labelFor(phase, idleLabel, elapsed)
  const isRunning = phase.kind === 'running'
  const showSuccess = phase.kind === 'success'

  // Screen-reader announcements (#80): the visible button label re-renders
  // every 100 ms with elapsed seconds — aria-live on the button would spam
  // screen readers through the whole 5–30 s proving stage. Instead an
  // sr-only role="status" region announces only stage TRANSITIONS (label
  // without the timer). Errors are announced by <SubmitError>'s
  // role="alert" — keep them out of this region to avoid double-speak.
  const announcement =
    phase.kind === 'running'
      ? `${STAGE_LABELS[phase.stage]} …`
      : phase.kind === 'success'
        ? 'ORDER PLACED'
        : ''

  return (
    <div className="flex flex-col">
      <span role="status" aria-atomic="true" className="sr-only">
        {announcement}
      </span>
      <button
        type="button"
        onClick={onClick}
        disabled={disabled || isRunning}
        aria-busy={isRunning || undefined}
        className={cn(
          'relative flex h-12 items-center justify-center px-8',
          'font-mono uppercase tracking-[0.15em] text-[11px] font-medium leading-none',
          'transition-[color,background-color,box-shadow] duration-150 ease-out',
          'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
          accent
            ? 'bg-brand-accent text-brand-on-accent hover:shadow-accent-glow'
            : 'bg-transparent text-brand-muted border border-brand-border shadow-[inset_0_0_0_1px_#0C0C12]',
          'disabled:cursor-not-allowed',
          accent
            ? 'disabled:bg-brand-border disabled:text-brand-muted disabled:shadow-none'
            : 'disabled:text-brand-muted'
        )}
      >
        <span className="block">{label}</span>
      </button>
      <ProgressBar phase={phase} success={showSuccess} provingPct={provingPct} />
    </div>
  )
}

function labelFor(phase: SubmissionPhase, idleLabel: string, elapsedSecs: number): string {
  switch (phase.kind) {
    case 'idle':
      return idleLabel
    case 'running': {
      const base = STAGE_LABELS[phase.stage]
      // Show real elapsed seconds once the stage start is known.
      return phase.stageStartedAtMs !== undefined
        ? `${base} · ${elapsedSecs.toFixed(1)}s …`
        : `${base} …`
    }
    case 'success':
      return 'ORDER PLACED'
    case 'error':
      return 'TRY AGAIN'
  }
}

function ProgressBar({
  phase,
  success,
  provingPct,
}: {
  phase: SubmissionPhase
  success: boolean
  provingPct?: number | null
}) {
  const width = progressWidth(phase, success, provingPct)
  const visible = phase.kind === 'running' || success
  return (
    // State-driven movement, not a hover affordance — both transitions
    // freeze under prefers-reduced-motion per the DESIGN.md motion
    // contract (#80); the bar still snaps to each stage's width.
    <div
      aria-hidden
      className={cn(
        'h-[2px] w-full overflow-hidden bg-brand-border/0',
        'transition-opacity duration-150 motion-reduce:transition-none',
        visible ? 'opacity-100' : 'opacity-0'
      )}
    >
      <div
        data-testid="place-progress"
        className="h-full bg-brand-accent transition-[width] duration-150 ease-out motion-reduce:transition-none"
        style={{ width: `${(width * 100).toFixed(2)}%` }}
      />
    </div>
  )
}

function progressWidth(
  phase: SubmissionPhase,
  success: boolean,
  provingPct?: number | null
): number {
  if (success) return 1
  if (phase.kind !== 'running') return 0
  // During proving, follow the real prover percentage when available.
  if (phase.stage === 'proving' && provingPct != null) {
    return Math.min(1, Math.max(0, provingPct / 100))
  }
  return phase.progress
}

export function StageReadout({ phase }: { phase: SubmissionPhase }) {
  if (phase.kind !== 'running') return null
  return (
    <span className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted">
      {STAGE_LABELS[phase.stage as SubmitStageId]} …
    </span>
  )
}

/**
 * Inline error area shown below the place button when a submission fails.
 * Specific message per tonic code; on 429 a retry-after hint; on 5xx a
 * collapsible technical block carrying the x-request-id (C7).
 */
export function SubmitError({ phase }: { phase: SubmissionPhase }) {
  if (phase.kind !== 'error') return null
  const detail = phase.detail
  const showTechnical = detail != null && detail.httpStatus != null && detail.httpStatus >= 500

  return (
    <div role="alert" className="flex flex-col gap-2 border border-brand-border px-3 py-2">
      <p className="font-mono text-body-sm text-brand-fg">{phase.message}</p>

      {detail?.retryAfter && (
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-brand-muted">
          Retry after {detail.retryAfter}s
        </p>
      )}

      {showTechnical && (
        <details className="font-mono text-[10px] text-brand-muted">
          <summary className="cursor-pointer uppercase tracking-[0.2em]">
            [ TECHNICAL DETAIL ]
          </summary>
          <dl className="mt-2 flex flex-col gap-1">
            <div className="flex gap-2">
              <dt className="text-brand-muted">request-id</dt>
              <dd className="text-brand-fg">{detail?.requestId ?? '—'}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="text-brand-muted">code</dt>
              <dd className="text-brand-fg">
                {detail?.codeName} ({detail?.httpStatus})
              </dd>
            </div>
            {detail?.serverMessage && (
              <div className="flex gap-2">
                <dt className="text-brand-muted">message</dt>
                <dd className="text-brand-fg">{detail.serverMessage}</dd>
              </div>
            )}
          </dl>
        </details>
      )}
    </div>
  )
}
