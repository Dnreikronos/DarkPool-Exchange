import * as React from 'react'

import { Skeleton } from './skeleton'

export const Default = () => (
  <div className="flex flex-col gap-2 w-72">
    <Skeleton className="h-3 w-3/4" />
    <Skeleton className="h-3 w-1/2" />
    <Skeleton className="h-3 w-5/6" />
  </div>
)

export const TableRows = () => (
  <div className="flex flex-col gap-1 w-80">
    {Array.from({ length: 6 }).map((_, i) => (
      <div key={i} className="flex gap-2">
        <Skeleton className="h-3 w-20" />
        <Skeleton className="h-3 w-16" />
        <Skeleton className="h-3 w-24" />
      </div>
    ))}
  </div>
)
