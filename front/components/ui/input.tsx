'use client'

import * as React from 'react'

import { cn } from './cn'

export type InputProps = React.InputHTMLAttributes<HTMLInputElement>

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        ref={ref}
        type={type}
        className={cn(
          'flex h-10 w-full bg-brand-surface px-3 py-2',
          'font-mono text-[12px] leading-[1.8] text-brand-fg',
          'border border-brand-border',
          'placeholder:text-brand-muted',
          'transition-colors duration-150 ease-out',
          'focus:outline-none focus:border-brand-accent',
          'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
          'disabled:cursor-not-allowed disabled:bg-brand-bg disabled:text-brand-muted',
          'file:bg-transparent file:border-0 file:text-brand-fg file:font-mono file:text-[12px]',
          className
        )}
        {...props}
      />
    )
  }
)
Input.displayName = 'Input'

export { Input }
