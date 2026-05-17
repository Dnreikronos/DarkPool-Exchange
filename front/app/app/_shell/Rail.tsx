'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'

type RailItemDef = {
  href: string
  label: string
  n: string
  external?: boolean
}

const RAIL_ITEMS: readonly RailItemDef[] = [
  { href: '/app/trade', label: 'TRADE', n: '01' },
  { href: '/app/portfolio', label: 'PORTFOLIO', n: '02' },
  { href: '/docs', label: 'DOCS', n: '03', external: true },
]

export function Rail() {
  const pathname = usePathname()
  return (
    <nav
      aria-label="Primary"
      className="hidden lg:flex fixed left-0 top-24 bottom-0 z-30 w-40 flex-col gap-1 border-r border-brand-border bg-brand-bg/95 backdrop-blur-sm py-6"
    >
      {RAIL_ITEMS.map((item) => {
        const active =
          !item.external &&
          (pathname === item.href || pathname?.startsWith(`${item.href}/`))
        return (
          <RailItem
            key={item.href}
            href={item.href}
            n={item.n}
            label={item.label}
            active={active}
            external={item.external}
          />
        )
      })}
    </nav>
  )
}

function RailItem({
  href,
  n,
  label,
  active,
  external,
}: {
  href: string
  n: string
  label: string
  active: boolean
  external?: boolean
}) {
  const itemClass = [
    'group flex items-center gap-3 px-4 py-3 transition-colors duration-150',
    'focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-[-2px] focus-visible:outline-brand-accent',
    active
      ? 'text-brand-fg'
      : 'text-brand-muted hover:text-brand-fg',
  ].join(' ')

  const numberClass = [
    'inline-flex h-[30px] w-[30px] items-center justify-center border font-mono text-label-md uppercase transition-colors duration-150',
    active
      ? 'border-brand-fg text-brand-fg'
      : 'border-brand-border text-brand-muted group-hover:border-brand-muted group-hover:text-brand-fg',
  ].join(' ')

  const labelClass = 'font-mono text-label-lg uppercase'

  if (external) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className={itemClass}
      >
        <span aria-hidden="true" className={numberClass}>
          {n}
        </span>
        <span className={labelClass}>{label}</span>
      </a>
    )
  }

  return (
    <Link
      href={href}
      aria-current={active ? 'page' : undefined}
      className={itemClass}
    >
      <span aria-hidden="true" className={numberClass}>
        {n}
      </span>
      <span className={labelClass}>{label}</span>
    </Link>
  )
}
