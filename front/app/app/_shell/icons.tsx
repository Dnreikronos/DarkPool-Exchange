// Hand-rolled icon set for the /app chrome. Each icon is drawn for a
// 1px stroke, square line caps, and currentColor — matching the
// updated DESIGN.md rule (icons may be used when monochrome, 1px
// stroke, sharp corners, drawn for function not decoration).
//
// The icons are NOT a generic library. They are tuned per-purpose:
// a `WalletIcon` shaped like a card-and-flap pouch, an `ArbitrumHex`
// that mirrors the Arbitrum brand glyph, and panel glyphs that hint
// at the shape of the data each panel will hold.

import type { SVGProps } from 'react'

const BASE: SVGProps<SVGSVGElement> = {
  width: 14,
  height: 14,
  viewBox: '0 0 16 16',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1,
  strokeLinecap: 'square',
  strokeLinejoin: 'miter',
  'aria-hidden': true,
}

export function WalletIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* card body */}
      <path d="M2 5 L14 5 L14 13 L2 13 Z" />
      {/* flap */}
      <path d="M2 5 L11 5 L11 3 L4 3 L2 5" />
      {/* coin slot */}
      <path d="M10 8 L13 8 L13 11 L10 11 Z" />
    </svg>
  )
}

export function ArbitrumHex(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* hexagon */}
      <path d="M8 2 L13 5 L13 11 L8 14 L3 11 L3 5 Z" />
      {/* inner A-mark suggestion */}
      <path d="M6 11 L8 6 L10 11" />
      <path d="M7 9.5 L9 9.5" />
    </svg>
  )
}

export function OrderbookGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* stacked horizontal bars of varying length — bids/asks rhythm */}
      <path d="M2 4 L11 4" />
      <path d="M2 7 L13 7" />
      <path d="M2 10 L9 10" />
      <path d="M2 13 L12 13" />
    </svg>
  )
}

export function ChartGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* candle-ish: three vertical sticks with mid-bodies */}
      <path d="M4 3 L4 13" />
      <path d="M3 6 L5 6 L5 10 L3 10 Z" />
      <path d="M8 4 L8 14" />
      <path d="M7 7 L9 7 L9 12 L7 12 Z" />
      <path d="M12 5 L12 12" />
      <path d="M11 8 L13 8 L13 11 L11 11 Z" />
    </svg>
  )
}

export function EntryGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* form rows */}
      <path d="M2 4 L7 4" />
      <path d="M2 4 L2 6 L14 6 L14 4" />
      <path d="M2 9 L7 9" />
      <path d="M2 9 L2 11 L14 11 L14 9" />
      {/* submit chip */}
      <path d="M9 13 L14 13" />
    </svg>
  )
}

export function TapeGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* paper-tape with notches */}
      <path d="M3 2 L13 2 L13 14 L3 14 Z" />
      <path d="M3 5 L13 5" />
      <path d="M3 8 L13 8" />
      <path d="M3 11 L13 11" />
      {/* tear marks left */}
      <path d="M3 3.5 L4 3.5" />
      <path d="M3 6.5 L4 6.5" />
      <path d="M3 9.5 L4 9.5" />
      <path d="M3 12.5 L4 12.5" />
    </svg>
  )
}

export function TokenETH(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* diamond + horizontal split — abstract ether */}
      <path d="M8 1 L13 8 L8 15 L3 8 Z" />
      <path d="M3 8 L13 8" />
    </svg>
  )
}

export function TradeGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* two horizontal arrows opposing — the swap motion */}
      <path d="M2 5 L14 5" />
      <path d="M11 2 L14 5 L11 8" />
      <path d="M14 11 L2 11" />
      <path d="M5 8 L2 11 L5 14" />
    </svg>
  )
}

export function PortfolioGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* folio with a tab and inner rules */}
      <path d="M2 5 L6 5 L7 7 L14 7 L14 13 L2 13 Z" />
      <path d="M5 10 L12 10" />
      <path d="M5 12 L10 12" />
    </svg>
  )
}

export function DocsGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...BASE} {...props}>
      {/* two stacked pages */}
      <path d="M5 2 L13 2 L13 12 L5 12 Z" />
      <path d="M3 4 L11 4 L11 14 L3 14 Z" />
      <path d="M5 7 L9 7" />
      <path d="M5 10 L8 10" />
    </svg>
  )
}

