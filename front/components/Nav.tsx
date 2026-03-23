export default function Nav() {
  return (
    <nav className="fixed top-0 left-0 right-0 z-50 flex items-center justify-between px-12 py-4 bg-brand-bg/80 backdrop-blur-sm border-b border-brand-border">
      <span className="font-display text-[20px] text-white tracking-wider">
        DARKPOOL
      </span>
      <div className="flex gap-6">
        <a
          href="#"
          className="font-mono text-[11px] text-brand-muted tracking-[0.15em] hover:text-white transition-colors duration-150"
        >
          PROTOCOL
        </a>
        <a
          href="#"
          className="font-mono text-[11px] text-brand-muted tracking-[0.15em] hover:text-white transition-colors duration-150"
        >
          DOCS
        </a>
        <a
          href="#"
          className="font-mono text-[11px] text-brand-accent tracking-[0.15em] hover:text-white transition-colors duration-150"
        >
          LAUNCH APP
        </a>
      </div>
    </nav>
  )
}
