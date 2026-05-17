'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'

const NAV_LINKS = [
  { href: '/app/trade', label: 'TRADE' },
  { href: '/app/portfolio', label: 'PORTFOLIO' },
] as const

export function PrimaryNav() {
  const pathname = usePathname()
  return (
    <nav aria-label="Primary" className="hidden md:flex items-center gap-6">
      {NAV_LINKS.map((link) => {
        const active =
          pathname === link.href || pathname?.startsWith(`${link.href}/`)
        const className = [
          'font-mono text-label-lg uppercase transition-colors duration-150',
          'focus:outline-none focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-brand-accent',
          active ? 'text-brand-fg' : 'text-brand-muted hover:text-brand-fg',
        ].join(' ')
        return (
          <Link
            key={link.href}
            href={link.href}
            aria-current={active ? 'page' : undefined}
            className={className}
          >
            [ {link.label} ]
          </Link>
        )
      })}
    </nav>
  )
}
