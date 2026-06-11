'use client'

import * as React from 'react'

import { cn } from '@/components/ui/cn'

import type { Stage } from '../../_lib/deposit/stage-machine'

export interface Step {
  /** Two-char zero-padded label, e.g. `01`, `02`. */
  index: string
  /** Uppercase tracked label, e.g. `APPROVE`, `DEPOSIT`. */
  label: string
}

type StepState = 'pending' | 'current' | 'done' | 'failed'

interface StepIndicatorProps {
  steps: readonly Step[]
  stage: Stage
  /** Index in `steps` that maps to the live stage (0-based). */
  currentIndex: number
}

/**
 * Numbered step row, matching the `step-node` token from DESIGN.md:
 * 30×30px outlined square, two-digit zero-padded number, tracked label
 * underneath. The single active step picks up the lime accent (this is
 * the modal's lime budget when an active CTA is not visible).
 */
export function StepIndicator({ steps, stage, currentIndex }: StepIndicatorProps) {
  return (
    <ol className="grid grid-cols-2 gap-3" aria-label="Transaction steps">
      {steps.map((step, i) => {
        const state: StepState =
          stage.kind === 'error' && i === currentIndex
            ? 'failed'
            : i < currentIndex
              ? 'done'
              : i === currentIndex && (stage.kind === 'approving' || stage.kind === 'submitting')
                ? 'current'
                : stage.kind === 'confirmed' && i <= currentIndex
                  ? 'done'
                  : 'pending'
        return (
          <StepCell
            key={step.index}
            step={step}
            state={state}
            phase={state === 'current' ? stage.phase : undefined}
          />
        )
      })}
    </ol>
  )
}

/** Status line copy for the single in-flight step. */
function currentStatus(phase: Stage['phase']): string {
  if (phase === 'signing') return '[ CONFIRM IN WALLET ]'
  if (phase === 'mining') return '[ CONFIRMING ]'
  return '· · ·'
}

function StepCell({
  step,
  state,
  phase,
}: {
  step: Step
  state: StepState
  phase?: Stage['phase']
}) {
  return (
    <li className="flex items-start gap-3">
      <span
        aria-hidden
        className={cn(
          'flex h-[30px] w-[30px] shrink-0 items-center justify-center border',
          'font-mono text-label-md uppercase tracking-label',
          state === 'current' && 'border-brand-accent text-brand-accent',
          state === 'done' && 'border-brand-border text-brand-fg',
          state === 'pending' && 'border-brand-border text-brand-muted',
          state === 'failed' && 'border-brand-border text-brand-muted'
        )}
      >
        {step.index}
      </span>
      <div className="min-w-0 flex-1 pt-1">
        <p
          className={cn(
            'font-mono text-label-md uppercase tracking-label',
            state === 'current' && 'text-brand-fg',
            state === 'done' && 'text-brand-fg',
            state === 'pending' && 'text-brand-muted',
            state === 'failed' && 'text-brand-muted'
          )}
        >
          {step.label}
        </p>
        <p
          aria-live={state === 'current' ? 'polite' : 'off'}
          className="font-mono text-label-sm uppercase tracking-label text-brand-muted"
        >
          {state === 'current' && currentStatus(phase)}
          {state === 'done' && '[ DONE ]'}
          {state === 'pending' && '[ PENDING ]'}
          {state === 'failed' && '[ FAILED ]'}
        </p>
      </div>
    </li>
  )
}
