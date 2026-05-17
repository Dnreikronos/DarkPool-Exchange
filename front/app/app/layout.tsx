import Link from 'next/link'
import { ConnectButton } from '@/components/trade/ConnectButton'
import { AuctionStrip } from './_shell/AuctionStrip'
import { CommandPrompt } from './_shell/CommandPrompt'
import { PairSelector } from './_shell/PairSelector'
import { Rail } from './_shell/Rail'

export default function AppLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <div className="relative min-h-screen bg-brand-bg text-brand-fg">
      {/* Scope-out the landing's global crosshair cursor inside /app
          routes. Higher specificity (`body *`) + `!important` beats the
          `* { cursor: crosshair !important }` rule in
          front/app/globals.css. */}
      <style
        dangerouslySetInnerHTML={{
          __html: 'body,body *{cursor:auto !important}',
        }}
      />
      <Banner />
      <Rail />
      <main className="pt-16 md:pt-24 lg:pl-40">{children}</main>
    </div>
  )
}

function Banner() {
  return (
    <header className="fixed inset-x-0 top-0 z-40 border-b border-brand-border bg-brand-bg/90 backdrop-blur-sm">
      <div className="flex h-16 items-center justify-between px-page-x-mobile sm:px-page-x-tablet lg:px-page-x-desktop">
        <div className="flex items-center gap-8">
          <Link
            href="/app/trade"
            className="font-display text-headline-md tracking-brand text-brand-fg"
          >
            DARKPOOL
          </Link>
          <AuctionStrip />
        </div>
        <div className="flex items-center gap-3 sm:gap-6">
          <div className="hidden sm:block">
            <PairSelector />
          </div>
          <div className="hidden md:block">
            <NetworkIndicator />
          </div>
          <ConnectButton />
        </div>
      </div>
      <div className="hidden md:block">
        <CommandPrompt />
      </div>
    </header>
  )
}

function NetworkIndicator() {
  return (
    <div
      aria-label="Network: Arbitrum, chain 42161, status offline"
      className="flex h-10 items-center gap-2 border border-brand-border2 px-3"
    >
      <span
        aria-hidden="true"
        className="h-[6px] w-[6px] bg-brand-muted"
        style={{ borderRadius: 0 }}
      />
      <span className="font-mono text-label-lg uppercase text-brand-muted">
        [ ARBITRUM · 42161 ]
      </span>
    </div>
  )
}
