'use client'

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import {
  ColorType,
  CrosshairMode,
  LineStyle,
  type IChartApi,
  type ISeriesApi,
  type Time,
  type UTCTimestamp,
  createChart,
} from 'lightweight-charts'

import type { AuctionSummary } from '@/lib/sdk/proto/darkpool/v1/darkpool_pb.js'
import { useMockStore } from '@/lib/mock-store'
import { cn } from '@/components/ui/cn'

import {
  TIMEFRAMES,
  TIMEFRAME_MS,
  type PricePoint,
  type Timeframe,
  selectAuctionsInWindow,
} from './selectors'

const COLORS = {
  bg: '#06060A',
  text: '#5A5A72',
  textStrong: '#FFFFFF',
  grid: '#1C1C26',
  line: '#FFFFFF',
  crosshair: '#2E2E3E',
} as const

const MIN_AUCTIONS_FOR_CHART = 2

const TIMEFRAME_LABEL: Record<Timeframe, string> = {
  '1m': '1M',
  '5m': '5M',
  '1h': '1H',
}

export interface PriceHistoryChartViewProps {
  points: PricePoint[]
  /** Anchor for the time scale's right edge. Falls back to the last point's time. */
  rightEdgeUnixSec?: number
}

/**
 * Pure renderer over lightweight-charts. Set `points` and the chart updates
 * in place — the chart instance survives across re-renders so there is no
 * flicker.
 */
export function PriceHistoryChartView({ points, rightEdgeUnixSec }: PriceHistoryChartViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const chartRef = useRef<IChartApi | null>(null)
  const seriesRef = useRef<ISeriesApi<'Line'> | null>(null)

  // Create once. The chart instance owns its DOM canvas and we hand it the
  // current data on every render via the second effect.
  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return

    const chart = createChart(container, {
      width: container.clientWidth,
      height: container.clientHeight,
      layout: {
        background: { type: ColorType.Solid, color: COLORS.bg },
        textColor: COLORS.text,
        fontFamily: 'var(--font-ibm-plex-mono), ui-monospace, monospace',
        fontSize: 10,
      },
      grid: {
        vertLines: { color: COLORS.grid, style: LineStyle.Solid, visible: true },
        horzLines: { color: COLORS.grid, style: LineStyle.Solid, visible: true },
      },
      rightPriceScale: {
        borderColor: COLORS.grid,
        scaleMargins: { top: 0.15, bottom: 0.1 },
      },
      timeScale: {
        borderColor: COLORS.grid,
        timeVisible: true,
        secondsVisible: true,
        rightOffset: 2,
        barSpacing: 8,
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: {
          color: COLORS.crosshair,
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: COLORS.bg,
        },
        horzLine: {
          color: COLORS.crosshair,
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: COLORS.bg,
        },
      },
      handleScroll: false,
      handleScale: false,
    })

    const series = chart.addLineSeries({
      color: COLORS.line,
      lineWidth: 1,
      lineStyle: LineStyle.Solid,
      priceLineVisible: true,
      priceLineColor: COLORS.crosshair,
      priceLineStyle: LineStyle.Dotted,
      priceLineWidth: 1,
      lastValueVisible: true,
      crosshairMarkerVisible: true,
      crosshairMarkerRadius: 3,
      crosshairMarkerBorderColor: COLORS.textStrong,
      crosshairMarkerBackgroundColor: COLORS.textStrong,
      priceFormat: { type: 'price', precision: 2, minMove: 0.01 },
    })

    chartRef.current = chart
    seriesRef.current = series

    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect
      chart.applyOptions({ width: Math.floor(width), height: Math.floor(height) })
    })
    ro.observe(container)

    return () => {
      ro.disconnect()
      chart.remove()
      chartRef.current = null
      seriesRef.current = null
    }
  }, [])

  // Push data on every render. setData is the documented update path for
  // bulk replacement and reuses the existing canvas — no flicker.
  useEffect(() => {
    const series = seriesRef.current
    if (!series) return
    series.setData(points.map((p) => ({ time: p.time as UTCTimestamp, value: p.value })))
    if (rightEdgeUnixSec !== undefined && chartRef.current && points.length > 0) {
      chartRef.current.timeScale().scrollToPosition(0, false)
    }
  }, [points, rightEdgeUnixSec])

  return <div ref={containerRef} className="h-full w-full" />
}

export interface PriceHistoryChartProps {
  /** Initial timeframe; defaults to '5m'. */
  defaultTimeframe?: Timeframe
  className?: string
  /**
   * Test/story injection: override the wall clock used to evaluate
   * the timeframe window. Defaults to the newest auction's timestamp,
   * falling back to `Date.now()`.
   */
  nowUnixSec?: number
  /**
   * Test/story injection: override the auctions source instead of reading
   * the live mock store.
   */
  auctionsOverride?: readonly AuctionSummary[]
}

/**
 * Clearing-price history backed by the mock store's `recentAuctions`. The
 * user toggles between 1m / 5m / 1h windows; below the {@link
 * MIN_AUCTIONS_FOR_CHART} threshold the chart yields to a brutalist empty
 * state so first-load isn't a blank rectangle.
 *
 * Fills the parent height via flex; `MIN_CANVAS_HEIGHT` is the floor so
 * an unsized container still renders something usable.
 */
export function PriceHistoryChart({
  defaultTimeframe = '5m',
  className,
  nowUnixSec,
  auctionsOverride,
}: PriceHistoryChartProps = {}) {
  const [timeframe, setTimeframe] = useState<Timeframe>(defaultTimeframe)
  const storeAuctions = useMockStore((s) => s.recentAuctions)
  const auctions = auctionsOverride ?? storeAuctions

  const anchorNow = useMemo(() => {
    if (nowUnixSec !== undefined) return nowUnixSec
    if (auctions.length > 0) return Number(auctions[0].timestampUnix)
    return Math.floor(Date.now() / 1000)
  }, [nowUnixSec, auctions])

  const points = useMemo(
    () => selectAuctionsInWindow(auctions, TIMEFRAME_MS[timeframe], anchorNow),
    [auctions, timeframe, anchorNow]
  )

  const isEmpty = points.length < MIN_AUCTIONS_FOR_CHART

  return (
    <figure
      className={cn('flex h-full flex-col', className)}
      aria-labelledby="price-history-caption"
    >
      <PriceHistoryHeader
        timeframe={timeframe}
        onChange={setTimeframe}
        lastPrice={points.length > 0 ? points[points.length - 1].value : null}
      />
      <div className="relative w-full flex-1 min-h-[200px]" data-empty={isEmpty || undefined}>
        {isEmpty ? (
          <PriceHistoryEmptyState count={auctions.length} />
        ) : (
          <PriceHistoryChartView points={points} rightEdgeUnixSec={anchorNow} />
        )}
      </div>
      <figcaption id="price-history-caption" className="sr-only">
        Clearing price history, {TIMEFRAME_LABEL[timeframe]} window, {points.length} prints.
      </figcaption>
    </figure>
  )
}

function PriceHistoryHeader({
  timeframe,
  onChange,
  lastPrice,
}: {
  timeframe: Timeframe
  onChange: (t: Timeframe) => void
  lastPrice: number | null
}) {
  return (
    <div className="flex items-baseline justify-between border-b border-brand-border px-4 py-2">
      <span className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted">
        [ CLEARING PRICE ]
      </span>
      <div className="flex items-center gap-4">
        <span className="font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted tabular-nums">
          {lastPrice !== null ? lastPrice.toFixed(2) : '—'}
        </span>
        <div role="group" aria-label="Timeframe" className="flex items-center gap-1">
          {TIMEFRAMES.map((tf) => (
            <TimeframeButton
              key={tf}
              active={tf === timeframe}
              onClick={() => onChange(tf)}
              label={TIMEFRAME_LABEL[tf]}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function TimeframeButton({
  active,
  onClick,
  label,
}: {
  active: boolean
  onClick: () => void
  label: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        'inline-flex h-7 min-w-[40px] items-center justify-center px-2',
        'font-mono text-label-sm font-medium uppercase tracking-[0.2em]',
        'border border-brand-border transition-colors duration-150 ease-out',
        'focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
        active
          ? 'border-brand-muted text-brand-fg'
          : 'text-brand-muted hover:text-brand-fg hover:border-brand-muted'
      )}
    >
      [ {label} ]
    </button>
  )
}

function PriceHistoryEmptyState({ count }: { count: number }) {
  const message = count === 0 ? '[ AWAITING FIRST AUCTION ]' : '[ NEED ≥ 2 AUCTIONS FOR CHART ]'
  return (
    <div
      role="status"
      className="absolute inset-0 flex items-center justify-center font-mono text-label-md uppercase tracking-[0.2em] text-brand-muted"
    >
      {message}
    </div>
  )
}

// Re-exports for callers that want to drive the chart from a parent state.
export { TIMEFRAMES, TIMEFRAME_LABEL }
export type { Time }
