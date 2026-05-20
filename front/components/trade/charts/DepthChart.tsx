'use client'

import { useMemo } from 'react'
import { AxisBottom } from '@visx/axis'
import { curveStepAfter, curveStepBefore } from '@visx/curve'
import { Group } from '@visx/group'
import { ParentSize } from '@visx/responsive'
import { scaleLinear } from '@visx/scale'
import { AreaClosed, Line } from '@visx/shape'

import { type DepthSeries, buildDepthSeries } from './selectors'
import { useMockStore } from '../../../lib/mock-store'
import { cn } from '../../ui/cn'

const COLORS = {
  bidStroke: '#FFFFFF',
  bidFill: '#FFFFFF',
  askStroke: '#5A5A72',
  askFill: '#5A5A72',
  axis: '#5A5A72',
  midLine: '#5A5A72',
  outline: '#1C1C26',
} as const

const MARGIN = { top: 12, right: 16, bottom: 28, left: 16 }
const MIN_CANVAS_HEIGHT = 160

export interface DepthChartViewProps {
  series: DepthSeries
  width: number
  height: number
}

/**
 * Pure SVG renderer. Exposed for Ladle stories and visual tests that pump
 * a fixed `DepthSeries` in without spinning up the mock store.
 */
export function DepthChartView({ series, width, height }: DepthChartViewProps) {
  const innerWidth = Math.max(0, width - MARGIN.left - MARGIN.right)
  const innerHeight = Math.max(0, height - MARGIN.top - MARGIN.bottom)

  // Reverse bids so the display order is ascending price (left → right),
  // matching how the staircase reads on the x-axis.
  const bidsAsc = useMemo(() => [...series.bids].reverse(), [series.bids])

  const xDomain = useMemo<[number, number]>(() => {
    const lowest = bidsAsc[0]?.price ?? series.asks[0]?.price ?? 0
    const highest =
      series.asks[series.asks.length - 1]?.price ?? bidsAsc[bidsAsc.length - 1]?.price ?? lowest
    return lowest === highest ? [lowest - 1, highest + 1] : [lowest, highest]
  }, [bidsAsc, series.asks])

  const xScale = useMemo(
    () => scaleLinear<number>({ domain: xDomain, range: [0, innerWidth], clamp: true }),
    [xDomain, innerWidth]
  )

  const yScale = useMemo(
    () =>
      scaleLinear<number>({
        domain: [0, series.maxCumulative === 0 ? 1 : series.maxCumulative * 1.05],
        range: [innerHeight, 0],
        clamp: true,
      }),
    [series.maxCumulative, innerHeight]
  )

  const isEmpty = series.bids.length === 0 && series.asks.length === 0

  if (innerWidth <= 0 || innerHeight <= 0) return null

  return (
    <svg
      width={width}
      height={height}
      role="img"
      aria-label={
        isEmpty
          ? 'Depth chart, no orderbook data yet'
          : `Depth chart, ${series.bids.length} bid levels and ${series.asks.length} ask levels`
      }
    >
      <Group left={MARGIN.left} top={MARGIN.top}>
        {bidsAsc.length > 0 && (
          <AreaClosed
            data={bidsAsc}
            x={(d) => xScale(d.price)}
            y={(d) => yScale(d.cumulative)}
            yScale={yScale}
            curve={curveStepBefore}
            stroke={COLORS.bidStroke}
            strokeOpacity={0.55}
            strokeWidth={1}
            fill={COLORS.bidFill}
            fillOpacity={0.1}
          />
        )}

        {series.asks.length > 0 && (
          <AreaClosed
            data={series.asks}
            x={(d) => xScale(d.price)}
            y={(d) => yScale(d.cumulative)}
            yScale={yScale}
            curve={curveStepAfter}
            stroke={COLORS.askStroke}
            strokeOpacity={0.85}
            strokeWidth={1}
            fill={COLORS.askFill}
            fillOpacity={0.22}
          />
        )}

        {series.midPrice !== null && (
          <Line
            from={{ x: xScale(series.midPrice), y: 0 }}
            to={{ x: xScale(series.midPrice), y: innerHeight }}
            stroke={COLORS.midLine}
            strokeOpacity={0.45}
            strokeDasharray="2 3"
            strokeWidth={1}
            shapeRendering="crispEdges"
          />
        )}

        <AxisBottom
          top={innerHeight}
          scale={xScale}
          numTicks={Math.max(2, Math.min(5, Math.floor(innerWidth / 80)))}
          stroke={COLORS.outline}
          tickStroke={COLORS.outline}
          tickLength={4}
          tickFormat={(d) => formatTick(Number(d))}
          tickLabelProps={() => ({
            fill: COLORS.axis,
            fontFamily: 'var(--font-ibm-plex-mono)',
            fontSize: 9,
            fontWeight: 500,
            letterSpacing: '0.2em',
            textAnchor: 'middle',
            dy: '0.6em',
            style: { textTransform: 'uppercase' },
          })}
        />
      </Group>
    </svg>
  )
}

function formatTick(n: number): string {
  if (!Number.isFinite(n)) return ''
  if (Math.abs(n) >= 10_000) return n.toLocaleString('en-US', { maximumFractionDigits: 0 })
  return n.toFixed(n >= 100 ? 0 : 2)
}

export interface DepthChartProps {
  className?: string
}

/**
 * Live depth chart driven by the mock store. Re-derives the cumulative
 * series on every orderbook update; visx SVG nodes are reused across
 * renders so the chart redraws in place without flicker.
 *
 * Fills the parent height via flex; `MIN_CANVAS_HEIGHT` is the floor so
 * an unsized container still renders something usable.
 */
export function DepthChart({ className }: DepthChartProps = {}) {
  const orderbook = useMockStore((s) => s.orderbook)
  const series = useMemo(() => buildDepthSeries(orderbook), [orderbook])
  const isEmpty = series.bids.length === 0 && series.asks.length === 0

  return (
    <figure className={cn('flex h-full flex-col', className)} aria-labelledby="depth-chart-caption">
      <DepthChartHeader series={series} />
      <div className="relative flex-1 min-h-[160px]">
        <ParentSize debounceTime={0}>
          {({ width, height }) => (
            <DepthChartView
              series={series}
              width={width}
              height={Math.max(MIN_CANVAS_HEIGHT, height)}
            />
          )}
        </ParentSize>
        {isEmpty && <DepthChartEmptyState />}
      </div>
      <figcaption id="depth-chart-caption" className="sr-only">
        Cumulative bid and ask depth for the active pair.
      </figcaption>
    </figure>
  )
}

function DepthChartHeader({ series }: { series: DepthSeries }) {
  return (
    <div className="flex items-baseline justify-between border-b border-brand-border px-4 py-2">
      <span className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted">
        [ DEPTH ]
      </span>
      <span className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted tabular-nums">
        {series.midPriceStr !== null ? `MID ${formatTick(Number(series.midPriceStr))}` : 'MID —'}
        {'  ·  '}
        {series.spread !== null ? `SPR ${series.spread.toFixed(2)}` : 'SPR —'}
      </span>
    </div>
  )
}

function DepthChartEmptyState() {
  return (
    <div className="absolute inset-0 flex items-center justify-center">
      <p
        role="status"
        className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted"
      >
        [ NO BOOK YET ]
      </p>
    </div>
  )
}
