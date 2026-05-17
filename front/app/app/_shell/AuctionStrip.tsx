export function AuctionStrip() {
  return (
    <div
      aria-label="Auction state"
      className="hidden md:flex items-center gap-3 font-mono text-label-md uppercase text-brand-muted"
    >
      <span>AUC ──</span>
      <span aria-hidden="true" className="text-brand-border2">
        ·
      </span>
      <span>BATCH ──</span>
      <span aria-hidden="true" className="text-brand-border2">
        ·
      </span>
      <span>IDLE</span>
    </div>
  )
}
