import * as React from 'react'

import { NumericText } from './NumericText'

export const SinglePrice = () => (
  <NumericText value="2418.1" kind="price" className="text-display-sm font-display" />
)

export const SizeAndUsd = () => (
  <div className="flex flex-col gap-2">
    <NumericText value="0.04210000" kind="size" />
    <NumericText value="2418.1" kind="usd" />
  </div>
)

export const ColumnAlignment = () => {
  const prices = ['2418.10', '2417.05', '12345.6789', '0.05', '1', '99999.5']
  return (
    <div className="inline-flex flex-col gap-1 border border-brand-border p-3 bg-brand-surface w-56">
      <div className="text-label-md font-mono uppercase tracking-[0.2em] text-brand-muted text-right pb-1 border-b border-brand-border">
        [ PRICE · USDC ]
      </div>
      {prices.map((p) => (
        <NumericText key={p} value={p} kind="price" className="w-full" />
      ))}
    </div>
  )
}

export const Sizes4dp = () => {
  const sizes = ['0.0421', '1.5', '12.3456', '12345.6789', '0.00001']
  return (
    <div className="inline-flex flex-col gap-1 border border-brand-border p-3 bg-brand-surface w-56">
      <div className="text-label-md font-mono uppercase tracking-[0.2em] text-brand-muted text-right pb-1 border-b border-brand-border">
        [ SIZE · WETH ]
      </div>
      {sizes.map((s) => (
        <NumericText key={s} value={s} kind="size" className="w-full" />
      ))}
    </div>
  )
}

export const Negative = () => (
  <div className="flex flex-col gap-2">
    <NumericText value="-128.42" kind="usd" />
    <NumericText value="-0.0042" kind="size" />
  </div>
)

export const Placeholder = () => (
  <div className="flex flex-col gap-2">
    <NumericText value="" kind="price" />
    <NumericText value="notanumber" kind="price" placeholder="N/A" />
  </div>
)

export const LeftAligned = () => <NumericText value="2418.1" kind="price" align="left" />
