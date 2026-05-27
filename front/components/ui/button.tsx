'use client'

import * as React from 'react'
import { Slot } from '@radix-ui/react-slot'
import { cva, type VariantProps } from 'class-variance-authority'

import { cn } from './cn'

const buttonVariants = cva(
  cn(
    'inline-flex items-center justify-center select-none',
    'font-mono uppercase tracking-[0.15em] text-[11px] font-medium leading-none',
    'transition-colors transition-shadow duration-150 ease-out',
    'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
    'disabled:pointer-events-none disabled:cursor-not-allowed'
  ),
  {
    variants: {
      variant: {
        primary: cn(
          'bg-brand-accent text-brand-on-accent',
          'hover:shadow-accent-glow',
          'disabled:bg-brand-border disabled:text-brand-muted disabled:shadow-none'
        ),
        // Engraved double-1px: an outer border + an inset shadow that
        // reads as a hairline 1px inside the outer edge. Hover lifts the
        // hairline; press inverts it.
        ghost: cn(
          'bg-transparent text-brand-muted border border-brand-border',
          'shadow-[inset_0_0_0_1px_#0C0C12]',
          'hover:text-brand-fg hover:border-brand-muted hover:shadow-[inset_0_0_0_1px_#1C1C26]',
          'active:shadow-[inset_0_0_0_1px_#2E2E3E]',
          'disabled:text-brand-muted disabled:border-brand-border disabled:hover:border-brand-border disabled:hover:text-brand-muted disabled:hover:shadow-[inset_0_0_0_1px_#0C0C12]'
        ),
      },
      size: {
        default: 'h-12 px-8',
        sm: 'h-8 px-4 text-[10px] tracking-[0.2em]',
      },
    },
    defaultVariants: {
      variant: 'primary',
      size: 'default',
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button'
    return (
      <Comp className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props} />
    )
  }
)
Button.displayName = 'Button'

export { Button, buttonVariants }
