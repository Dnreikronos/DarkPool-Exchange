import * as React from 'react'
import { create } from '@bufbuild/protobuf'

import {
  GetOrderBookResponseSchema,
  PriceLevelSchema,
} from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'
import type { GetOrderBookResponse, PriceLevel } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'

import { DepthChartView } from './DepthChart'
import { buildDepthSeries } from '../../_lib/charts/selectors'

function level(price: string, totalSize: string, orderCount = 1): PriceLevel {
  return create(PriceLevelSchema, { price, totalSize, orderCount })
}

function book(bids: PriceLevel[], asks: PriceLevel[]): GetOrderBookResponse {
  return create(GetOrderBookResponseSchema, { pair: 'ETH/USDC', bids, asks })
}

const FRAME = 'border border-brand-border bg-brand-surface p-4 w-[640px] h-[280px]'

export const Symmetric = () => {
  const series = buildDepthSeries(
    book(
      Array.from({ length: 12 }, (_, i) =>
        level((3000 - i).toString(), (0.4 + i * 0.18).toFixed(4))
      ),
      Array.from({ length: 12 }, (_, i) =>
        level((3001 + i).toString(), (0.4 + i * 0.18).toFixed(4))
      )
    )
  )
  return (
    <div className={FRAME}>
      <DepthChartView series={series} width={608} height={248} />
    </div>
  )
}

export const Asymmetric = () => {
  const series = buildDepthSeries(
    book(
      [level('2999', '2'), level('2998', '4'), level('2997', '1.5'), level('2995', '0.8')],
      [level('3001', '0.3'), level('3002', '0.6'), level('3005', '0.4')]
    )
  )
  return (
    <div className={FRAME}>
      <DepthChartView series={series} width={608} height={248} />
    </div>
  )
}

export const Sparse = () => {
  const series = buildDepthSeries(book([level('2999', '0.5')], [level('3010', '0.5')]))
  return (
    <div className={FRAME}>
      <DepthChartView series={series} width={608} height={248} />
    </div>
  )
}

export const Empty = () => {
  const series = buildDepthSeries(book([], []))
  return (
    <div className={FRAME}>
      <DepthChartView series={series} width={608} height={248} />
    </div>
  )
}

export const BidsOnly = () => {
  const series = buildDepthSeries(
    book(
      Array.from({ length: 6 }, (_, i) => level((3000 - i).toString(), '1')),
      []
    )
  )
  return (
    <div className={FRAME}>
      <DepthChartView series={series} width={608} height={248} />
    </div>
  )
}
