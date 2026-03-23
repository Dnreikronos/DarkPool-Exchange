export default function Footer() {
  return (
    <footer className="relative bg-brand-bg border-t border-brand-border">
      <div className="px-6 md:px-12 lg:px-20 md:pr-[300px] py-16 md:py-20">
        {/* Top row — brand left, links center */}
        <div className="flex flex-col md:flex-row gap-12 md:gap-20 mb-0">
          {/* Brand */}
          <div className="flex-shrink-0">
            <h3 className="font-display text-[clamp(32px,4vw,48px)] text-white leading-[0.95] mb-5">
              DARKPOOL
              <br />
              <span className="text-brand-accent">PROTOCOL</span>
            </h3>
            <p className="font-mono text-[12px] text-brand-muted leading-[1.8] max-w-[320px]">
              A decentralized exchange where orders stay private until
              settlement. Built with zero-knowledge proofs on Arbitrum.
            </p>
          </div>

          {/* Links — spread across center */}
          <div className="flex flex-wrap gap-x-16 gap-y-10">
            <div>
              <span className="font-mono text-[10px] text-brand-accent tracking-[0.2em] block mb-5">
                PROTOCOL
              </span>
              <ul className="space-y-3">
                {['Documentation', 'Whitepaper', 'Audit Report', 'GitHub'].map(
                  (item) => (
                    <li key={item}>
                      <a
                        href="#"
                        className="font-mono text-[12px] text-brand-muted hover:text-white transition-colors duration-150"
                      >
                        {item}
                      </a>
                    </li>
                  )
                )}
              </ul>
            </div>

            <div>
              <span className="font-mono text-[10px] text-brand-accent tracking-[0.2em] block mb-5">
                COMMUNITY
              </span>
              <ul className="space-y-3">
                {['Twitter', 'Discord', 'Telegram', 'Blog'].map((item) => (
                  <li key={item}>
                    <a
                      href="#"
                      className="font-mono text-[12px] text-brand-muted hover:text-white transition-colors duration-150"
                    >
                      {item}
                    </a>
                  </li>
                ))}
              </ul>
            </div>

            <div>
              <span className="font-mono text-[10px] text-brand-accent tracking-[0.2em] block mb-5">
                DEVELOPERS
              </span>
              <ul className="space-y-3">
                {['SDK', 'API Reference', 'Smart Contracts', 'Bug Bounty'].map(
                  (item) => (
                    <li key={item}>
                      <a
                        href="#"
                        className="font-mono text-[12px] text-brand-muted hover:text-white transition-colors duration-150"
                      >
                        {item}
                      </a>
                    </li>
                  )
                )}
              </ul>
            </div>
          </div>
        </div>
      </div>

      {/* Bottom bar */}
      <div className="border-t border-brand-border px-6 md:px-12 lg:px-20 md:pr-[300px] py-5">
        <div className="flex flex-col md:flex-row items-start md:items-center gap-3">
          <span className="font-mono text-[11px] text-brand-muted">
            © 2025 DarkPool Protocol
          </span>

          <span className="font-mono text-[11px] text-brand-border2 hidden md:inline">—</span>

          <div className="flex items-center gap-3">
            <span
              className="w-1.5 h-1.5 bg-brand-accent animate-blink inline-block"
              style={{ borderRadius: 0 }}
            />
            <span className="font-mono text-[11px] text-brand-muted">
              Mainnet: Offline
            </span>
            <span className="font-mono text-[11px] text-brand-border2">—</span>
            <span className="font-mono text-[11px] text-brand-accent">
              Testnet: Live
            </span>
          </div>
        </div>
      </div>
    </footer>
  )
}
