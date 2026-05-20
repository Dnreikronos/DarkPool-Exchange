'use client'

// Visual layer for the multi-stage submission. Renders the place-button
// label (which mutates through the stages) and the thin progress bar
// directly underneath the button — no modal stacking, per the
// DESIGN-INSPIRATIONS pattern "Submission: button label mutates through
// the 4 stages, with a thin tertiary progress bar underneath the button
// (height 2px, filling left to right). No modals."
//
// The component is presentational only. The state machine and the
// progress numbers come from useSubmitStages.ts; this file just maps
// them to DOM. F1.12 (#79) reuses the same stage labels so renaming is
// a coordinated change.

import * as React from 'react'

import { cn } from '../../ui/cn'

import { STAGE_LABELS, type SubmitStageId } from './policy'
import type { SubmissionPhase } from './useSubmitStages'

export interface PlaceButtonProps {
  /** What the button reads when idle (e.g. "BUY · WETH"). */
  idleLabel: string
  phase: SubmissionPhase
  disabled?: boolean
  onClick: () => void
  /**
   * When true, the button is rendered as the accent surface (lime).
   * The accent migrates here from the auction countdown when the form
   * is valid and not submitting — see DESIGN-INSPIRATIONS.md.
   */
  accent: boolean
}

export function PlaceButton({ idleLabel, phase, disabled, onClick, accent }: PlaceButtonProps) {
  const label = labelFor(phase, idleLabel)
  const isRunning = phase.kind === 'running'
  const showSuccess = phase.kind === 'success'

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={onClick}
        disabled={disabled || isRunning}
        aria-busy={isRunning || undefined}
        aria-live="polite"
        className={cn(
          'relative flex h-12 items-center justify-center px-8',
          'font-mono uppercase tracking-[0.15em] text-[11px] font-medium leading-none',
          // Custom transition-property so colors AND shadow both animate —
          // chaining `transition-colors transition-shadow` lets the second
          // class win and only shadow tweens.
          'transition-[color,background-color,box-shadow] duration-150 ease-out',
          'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
          accent
            ? 'bg-brand-accent text-brand-on-accent hover:shadow-accent-glow'
            : 'bg-transparent text-brand-muted border border-brand-border shadow-[inset_0_0_0_1px_#0C0C12]',
          // Hold ground on hover when not eligible — the form is the
          // signal of validity; the button itself shouldn't flicker.
          'disabled:cursor-not-allowed',
          accent
            ? 'disabled:bg-brand-border disabled:text-brand-muted disabled:shadow-none'
            : 'disabled:text-brand-muted'
        )}
      >
        <span className="block">{label}</span>
      </button>
      <ProgressBar phase={phase} success={showSuccess} />
    </div>
  )
}

function labelFor(phase: SubmissionPhase, idleLabel: string): string {
  switch (phase.kind) {
    case 'idle':
      return idleLabel
    case 'running':
      return `${STAGE_LABELS[phase.stage]} …`
    case 'success':
      return 'ORDER PLACED'
    case 'error':
      return 'TRY AGAIN'
  }
}

function ProgressBar({ phase, success }: { phase: SubmissionPhase; success: boolean }) {
  const width = progressWidth(phase, success)
  const visible = phase.kind === 'running' || success
  return (
    <div
      aria-hidden
      className={cn(
        'h-[2px] w-full overflow-hidden bg-brand-border/0',
        'transition-opacity duration-150',
        visible ? 'opacity-100' : 'opacity-0'
      )}
    >
      <div
        data-testid="place-progress"
        className="h-full bg-brand-accent transition-[width] duration-150 ease-out"
        style={{ width: `${(width * 100).toFixed(2)}%` }}
      />
    </div>
  )
}

function progressWidth(phase: SubmissionPhase, success: boolean): number {
  if (success) return 1
  if (phase.kind === 'running') return phase.progress
  return 0
}

export function StageReadout({ phase }: { phase: SubmissionPhase }) {
  // Optional out-of-band readout for cases that want the textual stage
  // label outside the button (Storybook variants, debugging surfaces).
  if (phase.kind !== 'running') return null
  return (
    <span className="font-mono text-[10px] font-medium uppercase tracking-[0.2em] text-brand-muted">
      {STAGE_LABELS[phase.stage as SubmitStageId]} …
    </span>
  )
}
