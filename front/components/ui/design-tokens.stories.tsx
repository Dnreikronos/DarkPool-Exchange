import * as React from 'react'

const swatches: { name: string; hex: string; on: string }[] = [
  { name: 'brand.bg', hex: '#06060A', on: '#FFFFFF' },
  { name: 'brand.surface', hex: '#0C0C12', on: '#FFFFFF' },
  { name: 'brand.border', hex: '#1C1C26', on: '#FFFFFF' },
  { name: 'brand.border2', hex: '#2E2E3E', on: '#FFFFFF' },
  { name: 'brand.muted', hex: '#5A5A72', on: '#FFFFFF' },
  { name: 'brand.fg', hex: '#FFFFFF', on: '#06060A' },
  { name: 'brand.accent', hex: '#D4FF00', on: '#06060A' },
]

const typeScale: { name: string; cls: string; sample: string; family: string }[] = [
  {
    name: 'display-xl 148/0.88',
    cls: 'text-display-xl font-display uppercase',
    sample: 'DARK',
    family: 'Bebas Neue',
  },
  {
    name: 'display-lg 72/0.92',
    cls: 'text-display-lg font-display uppercase',
    sample: 'DARK POOL',
    family: 'Bebas Neue',
  },
  {
    name: 'display-md 48/0.95',
    cls: 'text-display-md font-display uppercase',
    sample: 'AUCTION 1042',
    family: 'Bebas Neue',
  },
  {
    name: 'display-sm 24/1',
    cls: 'text-display-sm font-display uppercase',
    sample: 'LAST 2,418.10',
    family: 'Bebas Neue',
  },
  {
    name: 'headline-md 20/1',
    cls: 'text-headline-md font-display uppercase',
    sample: 'DarkPool',
    family: 'Bebas Neue',
  },
  {
    name: 'body-lg 14/1.85',
    cls: 'text-body-lg font-mono',
    sample: 'Orders are encrypted to the operator.',
    family: 'IBM Plex Mono',
  },
  {
    name: 'body-md 12/1.8',
    cls: 'text-body-md font-mono',
    sample: 'Submitting…',
    family: 'IBM Plex Mono',
  },
  {
    name: 'body-sm 11/1.75',
    cls: 'text-body-sm font-mono',
    sample: '0.0421  2418.10  USDC',
    family: 'IBM Plex Mono',
  },
  {
    name: 'label-lg 11/0.15em',
    cls: 'text-label-lg font-mono uppercase',
    sample: '[ PLACE ORDER ]',
    family: 'IBM Plex Mono',
  },
  {
    name: 'label-md 10/0.2em',
    cls: 'text-label-md font-mono uppercase',
    sample: 'PROTOCOL v0.1',
    family: 'IBM Plex Mono',
  },
  {
    name: 'label-sm 8/0.2em',
    cls: 'text-label-sm font-mono uppercase',
    sample: 'ARBITRUM · 42161',
    family: 'IBM Plex Mono',
  },
]

export const Overview = () => (
  <div className="flex flex-col gap-12 max-w-4xl">
    <section>
      <h2 className="text-label-md font-mono uppercase text-brand-muted mb-4">Palette</h2>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
        {swatches.map((s) => (
          <div
            key={s.name}
            className="flex items-center justify-between border border-brand-border px-4 py-3"
            style={{ background: s.hex, color: s.on }}
          >
            <span className="font-mono text-[11px] uppercase tracking-[0.15em]">{s.name}</span>
            <span className="font-mono text-[11px]">{s.hex}</span>
          </div>
        ))}
      </div>
    </section>

    <section>
      <h2 className="text-label-md font-mono uppercase text-brand-muted mb-4">Typography</h2>
      <div className="flex flex-col gap-6">
        {typeScale.map((t) => (
          <div key={t.name} className="flex flex-col gap-1 border-l border-brand-border pl-4">
            <span className="text-label-sm font-mono uppercase text-brand-muted">
              {t.name} · {t.family}
            </span>
            <span className={t.cls + ' text-brand-fg'}>{t.sample}</span>
          </div>
        ))}
      </div>
    </section>

    <section>
      <h2 className="text-label-md font-mono uppercase text-brand-muted mb-4">Radius</h2>
      <div className="flex gap-2">
        {['rounded', 'rounded-sm', 'rounded-md', 'rounded-lg', 'rounded-xl', 'rounded-full'].map(
          (cls) => (
            <div
              key={cls}
              className={
                cls +
                ' w-16 h-16 bg-brand-surface border border-brand-border flex items-center justify-center'
              }
            >
              <span className="text-label-sm font-mono uppercase text-brand-muted">
                {cls.replace('rounded-', '')}
              </span>
            </div>
          )
        )}
      </div>
      <p className="text-body-sm font-mono text-brand-muted mt-3">
        All radii collapse to 0. DESIGN.md hard rule.
      </p>
    </section>
  </div>
)
