import * as React from 'react'

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './tooltip'

export const Default = () => (
  <TooltipProvider delayDuration={150}>
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="text-body-sm font-mono uppercase tracking-[0.15em] text-brand-muted underline decoration-dotted underline-offset-4 cursor-help">
          [ PROOF ]
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">VERIFIED — GROTH16 BN254</TooltipContent>
    </Tooltip>
  </TooltipProvider>
)

export const Sides = () => (
  <TooltipProvider delayDuration={150}>
    <div className="flex flex-wrap gap-12">
      {(['top', 'right', 'bottom', 'left'] as const).map((side) => (
        <Tooltip key={side}>
          <TooltipTrigger asChild>
            <span className="text-body-sm font-mono uppercase tracking-[0.15em] text-brand-muted underline decoration-dotted underline-offset-4 cursor-help">
              [ {side.toUpperCase()} ]
            </span>
          </TooltipTrigger>
          <TooltipContent side={side}>OPENED {side.toUpperCase()}</TooltipContent>
        </Tooltip>
      ))}
    </div>
  </TooltipProvider>
)
