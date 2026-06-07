import Link from 'next/link'
import { Toaster } from '@/components/ui/toaster'
import { DarkPoolClientProvider } from '@/lib/api-client'
import { WalletProviders } from '@/lib/wallet'
import { ConnectButton } from './_components/ConnectButton'
import { HistoryBoot } from './_components/HistoryBoot'
import { OnboardingMount } from './_components/onboarding'
import { AuctionStrip } from './_shell/AuctionStrip'
import { ArbitrumHex } from './_shell/icons'
import { PairSelector } from './_shell/PairSelector'
import { Rail } from './_shell/Rail'

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <WalletProviders>
      <DarkPoolClientProvider>
        <div className="relative min-h-screen bg-brand-bg text-brand-fg">
          {/* Scope-out the landing's global crosshair cursor inside /app
            routes. Higher specificity (`body *`) + `!important` beats
            the `* { cursor: crosshair !important }` rule in
            front/app/globals.css. */}
          <style
            dangerouslySetInnerHTML={{
              __html:
                'body{cursor:default !important}' +
                'body *{cursor:inherit !important}' +
                'body a[href],body button,body summary,body [role=button],body [role=link],body [role=option],body [role=tab]{cursor:pointer !important}' +
                'body button[disabled],body [aria-disabled=true]{cursor:not-allowed !important}' +
                'body input,body textarea,body [contenteditable=true]{cursor:text !important}',
            }}
          />
          <Banner />
          <Rail />
          <main className="pt-16 lg:pl-56">{children}</main>
          <OnboardingMount />
          <HistoryBoot />
          <Toaster />
        </div>
      </DarkPoolClientProvider>
    </WalletProviders>
  )
}

function Banner() {
  return (
    <header className="fixed inset-x-0 top-0 z-40 flex h-16 items-center justify-between border-b border-brand-border bg-brand-bg/90 px-4 backdrop-blur-sm lg:px-6">
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
    </header>
  )
}

function NetworkIndicator() {
  return (
    <div
      aria-label="Network: Arbitrum, chain 42161, status offline"
      className="flex h-10 items-center gap-2 px-2"
    >
      <ArbitrumHex className="text-brand-muted" />
      <span className="font-mono text-label-lg uppercase text-brand-muted">ARBITRUM · 42161</span>
    </div>
  )
}
