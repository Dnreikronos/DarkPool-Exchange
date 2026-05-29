'use client'

import * as React from 'react'
import * as DialogPrimitive from '@radix-ui/react-dialog'

import { Dialog, DialogContent } from '@/components/ui/dialog'
import { cn } from '@/components/ui/cn'

import {
  ONBOARDING_BACK_LABEL,
  ONBOARDING_DIALOG_DESCRIPTION,
  ONBOARDING_DIALOG_TITLE,
  ONBOARDING_DONE_LABEL,
  ONBOARDING_NEXT_LABEL,
  ONBOARDING_STEPS,
  type OnboardingStep,
} from './copy'

/**
 * Pure step indicator (`01 02 03`). The current step renders in the
 * lime accent — this is the dialog's single accent budget. Past steps
 * read as completed (`primary`); future steps read as inert (`muted`).
 */
function StepIndicator({ step }: { step: number }) {
  return (
    <ol aria-label="Onboarding progress" className="flex items-center gap-2">
      {ONBOARDING_STEPS.map((s, i) => {
        const status = i === step ? 'current' : i < step ? 'past' : 'future'
        return (
          <li
            key={s.id}
            aria-current={status === 'current' ? 'step' : undefined}
            className={cn(
              'flex h-[30px] min-w-[30px] items-center justify-center border px-2 font-mono text-label-md uppercase tracking-labelWide',
              status === 'current' && 'border-brand-accent text-brand-accent',
              status === 'past' && 'border-brand-border2 text-brand-fg',
              status === 'future' && 'border-brand-border text-brand-muted'
            )}
          >
            {s.id}
          </li>
        )
      })}
    </ol>
  )
}

function StepBody({ step }: { step: OnboardingStep }) {
  return (
    <div className="flex flex-col gap-4">
      <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
        {step.kicker}
      </span>
      <h2 className="font-display text-display-sm uppercase leading-none text-brand-fg">
        {step.title}
      </h2>
      <div className="flex flex-col gap-3">
        {step.body.map((p, i) => (
          <p key={i} className="font-mono text-body-md text-brand-fg">
            {p}
          </p>
        ))}
      </div>
      {step.meta ? (
        <p className="font-mono text-body-sm uppercase tracking-label text-brand-muted">
          {step.meta}
        </p>
      ) : null}
    </div>
  )
}

export interface OnboardingPanelProps {
  /** 0-indexed current step. */
  step: number
  onBack: () => void
  onNext: () => void
  onDismiss: () => void
  /** Total steps. Defaults to ONBOARDING_STEPS.length. */
  totalSteps?: number
}

/**
 * The dialog body, without the Radix `<Dialog>` chrome. Exported so
 * tests can render it via `renderToStaticMarkup` — Radix's Portal-based
 * `<DialogContent>` emits no markup in SSR, so the dialog wrapper would
 * leave the test with an empty string. The portal-free panel is also
 * usable in Ladle stories that just want to inspect the layout.
 */
export function OnboardingPanel({
  step,
  onBack,
  onNext,
  onDismiss,
  totalSteps = ONBOARDING_STEPS.length,
}: OnboardingPanelProps) {
  const safeStep = Math.max(0, Math.min(step, totalSteps - 1))
  const current = ONBOARDING_STEPS[safeStep]
  const isFirst = safeStep === 0
  const isLast = safeStep === totalSteps - 1
  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-3">
        <span className="font-mono text-label-md uppercase tracking-labelWide text-brand-muted">
          {ONBOARDING_DIALOG_TITLE}
        </span>
        <StepIndicator step={safeStep} />
      </header>
      <StepBody step={current} />
      <footer className="flex items-center justify-between gap-3 border-t border-brand-border pt-5">
        <button
          type="button"
          onClick={onBack}
          disabled={isFirst}
          className={cn(
            'border border-brand-border2 px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-fg transition-colors',
            'hover:border-brand-fg focus-visible:border-brand-fg focus-visible:outline-none',
            'disabled:cursor-not-allowed disabled:border-brand-border disabled:text-brand-muted'
          )}
        >
          {ONBOARDING_BACK_LABEL}
        </button>
        {isLast ? (
          <button
            type="button"
            onClick={onDismiss}
            className={cn(
              'bg-brand-accent px-6 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-on-accent',
              'transition-shadow hover:shadow-accent-glow focus-visible:shadow-accent-glow focus-visible:outline-none'
            )}
          >
            {ONBOARDING_DONE_LABEL}
          </button>
        ) : (
          <button
            type="button"
            onClick={onNext}
            className={cn(
              'border border-brand-border2 px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-fg transition-colors',
              'hover:border-brand-fg focus-visible:border-brand-fg focus-visible:outline-none'
            )}
          >
            {ONBOARDING_NEXT_LABEL}
          </button>
        )}
      </footer>
    </div>
  )
}

export interface OnboardingDialogProps {
  /** Controlled open flag. */
  open: boolean
  /** Called when the user dismisses (close icon, `[ START TRADING ]`, or Esc). */
  onDismiss: () => void
}

/**
 * Radix-wrapped onboarding modal. Renders only when `open` is true; the
 * Portal mounts under the document body, so this component is
 * pre-hydration safe (the `OnboardingMount` host gates it on
 * `isReady`).
 */
export function OnboardingDialog({ open, onDismiss }: OnboardingDialogProps) {
  const [step, setStep] = React.useState(0)
  const totalSteps = ONBOARDING_STEPS.length

  // Reset to step 0 each time the dialog opens so a re-trigger starts
  // from the beginning. We intentionally do NOT reset on close so the
  // final-screen exit animation can play with the right step.
  React.useEffect(() => {
    if (open) setStep(0)
  }, [open])

  const handleOpenChange = React.useCallback(
    (next: boolean) => {
      if (!next) onDismiss()
    },
    [onDismiss]
  )

  const handleBack = React.useCallback(() => {
    setStep((s) => Math.max(0, s - 1))
  }, [])
  const handleNext = React.useCallback(() => {
    setStep((s) => Math.min(totalSteps - 1, s + 1))
  }, [totalSteps])

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        aria-label={ONBOARDING_DIALOG_TITLE}
        className="max-w-xl border border-brand-border"
      >
        <DialogPrimitive.Title className="sr-only">{ONBOARDING_DIALOG_TITLE}</DialogPrimitive.Title>
        <DialogPrimitive.Description className="sr-only">
          {ONBOARDING_DIALOG_DESCRIPTION}
        </DialogPrimitive.Description>
        <OnboardingPanel
          step={step}
          onBack={handleBack}
          onNext={handleNext}
          onDismiss={onDismiss}
          totalSteps={totalSteps}
        />
      </DialogContent>
    </Dialog>
  )
}
