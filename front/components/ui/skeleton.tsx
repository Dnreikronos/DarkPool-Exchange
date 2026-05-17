'use client'

import * as React from 'react'

import { cn } from './cn'

export type SkeletonProps = React.HTMLAttributes<HTMLDivElement>

function Skeleton({ className, ...props }: SkeletonProps) {
  return (
    <div
      className={cn('bg-brand-border animate-pulse motion-reduce:animate-none', className)}
      aria-hidden="true"
      {...props}
    />
  )
}

export { Skeleton }
