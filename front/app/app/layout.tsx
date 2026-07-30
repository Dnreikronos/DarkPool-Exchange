import Link from 'next/link'
import { Toaster } from '@/components/ui/toaster'
import { DarkPoolClientProvider } from '@/lib/api-client'
import { WalletProviders } from '@/lib/wallet'
import { ConnectButton } from './_components/ConnectButton'
import { HistoryBoot } from './_components/HistoryBoot'
import { OnboardingMount } from './_components/onboarding'
import { SettlementWatcher } from './_components/SettlementWatcher'
import { AuctionStrip } from './_shell/AuctionStrip'
import { NetworkIndicator } from './_shell/NetworkIndicator'
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
          {/* Keyboard users land on the banner/rail chrome first — give
              them a one-Tab bypass straight to the panels (#80). Visible
              only while focused; styled as a bracketed mono tag per
              DESIGN.md (zero radius, 1px outline, lime focus ring). */}
          <a
            href="#main"
            className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-50 focus:border focus:border-brand-border focus:bg-brand-bg focus:px-4 focus:py-2 focus:font-mono focus:text-label-lg focus:uppercase focus:tracking-label focus:text-brand-fg focus:outline focus:outline-1 focus:outline-offset-2 focus:outline-brand-accent"
          >
            [ SKIP TO CONTENT ]
          </a>
          <Banner />
          <Rail />
          {/* tabIndex={-1} lets the skip link programmatically focus the
              region on browsers that don't move sequential focus to
              fragment targets. */}
          <main id="main" tabIndex={-1} className="pt-16 lg:pl-56 focus:outline-none">
            {children}
          </main>
          <OnboardingMount />
          <HistoryBoot />
          <SettlementWatcher />
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
