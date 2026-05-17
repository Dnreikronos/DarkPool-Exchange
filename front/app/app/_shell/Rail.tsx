'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { CommandPrompt } from './CommandPrompt'

type RailItemDef = {
  href: string
  label: string
  n: string
  kbd?: string
  external?: boolean
}

const RAIL_ITEMS: readonly RailItemDef[] = [
  { href: '/app/trade', label: 'TRADE', n: '01', kbd: '⌘1' },
  { href: '/app/portfolio', label: 'PORTFOLIO', n: '02', kbd: '⌘2' },
  { href: '/docs', label: 'DOCS', n: '03', external: true },
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
        <h2 className="font-mono text-label-md uppercase text-brand-muted">
          NAVIGATE
        </h2>
      </div>
      <ul className="flex flex-col gap-px">
        {RAIL_ITEMS.map((item) => {
          const active =
            !item.external &&
            (pathname === item.href || pathname?.startsWith(`${item.href}/`))
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
      <span className="font-mono text-label-md uppercase text-brand-muted">
        {label}
      </span>
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

function RailItem({
  item,
  active,
}: {
  item: RailItemDef
  active: boolean
}) {
  const rowClass = [
    'group relative flex items-center gap-3 px-4 py-3 transition-colors duration-150',
    'focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent',
    active
      ? 'bg-brand-surface text-brand-fg'
      : 'text-brand-muted hover:bg-brand-surface/60 hover:text-brand-fg',
  ].join(' ')

  const numberClass = [
    'inline-flex h-[30px] w-[30px] flex-none items-center justify-center border font-mono text-label-md uppercase transition-colors duration-150',
    active
      ? 'border-brand-accent text-brand-accent'
      : 'border-brand-border text-brand-muted group-hover:border-brand-muted group-hover:text-brand-fg',
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

  if (item.external) {
    return (
      <li>
        <a
          href={item.href}
          target="_blank"
          rel="noopener noreferrer"
          className={rowClass}
        >
          <span aria-hidden="true" className={numberClass}>
            {item.n}
          </span>
          <span className={labelClass}>{item.label}</span>
          {suffix}
        </a>
      </li>
    )
  }

  return (
    <li>
      <Link
        href={item.href}
        aria-current={active ? 'page' : undefined}
        className={rowClass}
      >
        <span aria-hidden="true" className={numberClass}>
          {item.n}
        </span>
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
