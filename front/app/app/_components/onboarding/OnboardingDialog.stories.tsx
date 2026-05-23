import * as React from 'react'

import { OnboardingDialog, OnboardingPanel } from './OnboardingDialog'

// Ladle: the panel renders inline (no portal). Use this view to inspect
// the layout, step-indicator accent, and footer button states at each
// step without involving Radix's Portal.
export const PanelStep01 = () => (
  <div className="max-w-xl border border-brand-border bg-brand-surface p-8">
    <OnboardingPanel
      step={0}
      onBack={() => undefined}
      onNext={() => undefined}
      onDismiss={() => undefined}
    />
  </div>
)

export const PanelStep02 = () => (
  <div className="max-w-xl border border-brand-border bg-brand-surface p-8">
    <OnboardingPanel
      step={1}
      onBack={() => undefined}
      onNext={() => undefined}
      onDismiss={() => undefined}
    />
  </div>
)

export const PanelStep03 = () => (
  <div className="max-w-xl border border-brand-border bg-brand-surface p-8">
    <OnboardingPanel
      step={2}
      onBack={() => undefined}
      onNext={() => undefined}
      onDismiss={() => undefined}
    />
  </div>
)

// Full Radix dialog story — drives the open state from a button so the
// portal + overlay + close affordance all render in their final form.
export const DialogInteractive = () => {
  const [open, setOpen] = React.useState(true)
  return (
    <div className="flex h-screen items-center justify-center bg-brand-bg">
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="border border-brand-border2 px-4 py-2 font-mono text-label-md uppercase tracking-labelWide text-brand-fg hover:border-brand-fg"
      >
        [ OPEN ONBOARDING ]
      </button>
      <OnboardingDialog open={open} onDismiss={() => setOpen(false)} />
    </div>
  )
}
