export default function Ticker() {
  const text =
    'ZK-SNARK VERIFIED · BATCH SETTLEMENT · 100K ORDERS/SEC · P99 < 1MS · MEV EXPOSURE: ZERO · '

  return (
    <div className="border-b border-t border-brand-border py-2.5 overflow-hidden w-screen relative left-1/2 -translate-x-1/2">
      <div className="flex whitespace-nowrap animate-marquee">
        <span className="font-mono text-[10px] text-brand-accent tracking-[0.15em]">
          {text}{text}{text}{text}
        </span>
        <span className="font-mono text-[10px] text-brand-accent tracking-[0.15em]">
          {text}{text}{text}{text}
        </span>
      </div>
    </div>
  )
}
