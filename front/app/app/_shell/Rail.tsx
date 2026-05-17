'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { type ComponentType, type SVGProps } from 'react'
import { CommandPrompt } from './CommandPrompt'
import { DocsGlyph, PortfolioGlyph, TradeGlyph } from './icons'

type Glyph = ComponentType<SVGProps<SVGSVGElement>>

type RailItemDef = {
  href: string
  label: string
  icon: Glyph
  kbd?: string
  external?: boolean
}

const RAIL_ITEMS: readonly RailItemDef[] = [
  { href: '/app/trade', label: 'TRADE', icon: TradeGlyph, kbd: '⌘1' },
  {
    href: '/app/portfolio',
    label: 'PORTFOLIO',
    icon: PortfolioGlyph,
    kbd: '⌘2',
  },
  { href: '/docs', label: 'DOCS', icon: DocsGlyph, external: true },
]

export function Rail() {
  const pathname = usePathname()
  return (
    <nav
      aria-label="Primary"
      className="hidden lg:flex fixed left-0 top-16 bottom-0 z-30 w-56 flex-col border-r border-brand-border bg-brand-bg"
    >
      <StatusZone label="OPERATOR" value="IDLE" />

      <div className="px-4 pt-6 pb-3">
        <h2 className="font-mono text-label-md uppercase text-brand-muted">NAVIGATE</h2>
      </div>
      <ul className="flex flex-col gap-px">
        {RAIL_ITEMS.map((item) => {
          const active =
            !item.external && (pathname === item.href || pathname?.startsWith(`${item.href}/`))
          return <RailItem key={item.href} item={item} active={active} />
        })}
      </ul>

      <div className="mt-auto">
        <CommandPrompt />
        <StatusZone label="BATCH ──" value="IDLE" border="t" />
      </div>
    </nav>
  )
}

function StatusZone({
  label,
  value,
  border = 'b',
}: {
  label: string
  value: string
  border?: 'b' | 't'
}) {
  const borderClass = border === 't' ? 'border-t' : 'border-b'
  return (
    <div
      className={`flex h-9 items-center justify-between ${borderClass} border-brand-border px-4`}
    >
      <span className="font-mono text-label-md uppercase text-brand-muted">{label}</span>
      <span className="flex items-center gap-2 font-mono text-label-md uppercase text-brand-muted">
        <span
          aria-hidden="true"
          className="h-[6px] w-[6px] bg-brand-muted"
          style={{ borderRadius: 0 }}
        />
        {value}
      </span>
    </div>
  )
}

function RailItem({ item, active }: { item: RailItemDef; active: boolean }) {
  const rowClass = [
    'group relative flex items-center gap-3 px-4 py-3 transition-colors duration-150',
    'focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent',
    active
      ? 'bg-brand-surface text-brand-fg'
      : 'text-brand-muted hover:bg-brand-surface/60 hover:text-brand-fg',
  ].join(' ')

  const iconClass = [
    'flex-none transition-colors duration-150',
    active ? 'text-brand-accent' : 'text-brand-muted group-hover:text-brand-fg',
  ].join(' ')

  const labelClass = 'flex-1 font-mono text-label-lg uppercase'

  const kbdClass = active
    ? 'font-mono text-label-md text-brand-muted'
    : 'font-mono text-label-md text-brand-muted opacity-0 transition-opacity group-hover:opacity-100'

  const suffix = item.external ? (
    <ExternalGlyph aria-hidden="true" className="text-brand-muted" />
  ) : item.kbd ? (
    <kbd className={kbdClass}>{item.kbd}</kbd>
  ) : null

  const Icon = item.icon

  if (item.external) {
    return (
      <li>
        <a href={item.href} target="_blank" rel="noopener noreferrer" className={rowClass}>
          <Icon aria-hidden="true" className={iconClass} />
          <span className={labelClass}>{item.label}</span>
          {suffix}
        </a>
      </li>
    )
  }

  return (
    <li>
      <Link href={item.href} aria-current={active ? 'page' : undefined} className={rowClass}>
        <Icon aria-hidden="true" className={iconClass} />
        <span className={labelClass}>{item.label}</span>
        {suffix}
      </Link>
    </li>
  )
}

function ExternalGlyph(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      strokeLinecap="square"
      {...props}
    >
      <path d="M5 11 L11 5" />
      <path d="M6 5 L11 5 L11 10" />
    </svg>
  )
}
