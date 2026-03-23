'use client'

const orders = [
  { id: '0xc3f1', type: 'BID', status: 'COMMITTED', proof: true, batch: 47 },
  { id: '0x9e2b', type: 'ASK', status: 'PROVING', proof: false, batch: null },
  { id: '0x71fa', type: 'BID', status: 'MATCHED', proof: true, batch: 47 },
  { id: '0xd4e0', type: 'ASK', status: 'SETTLED', proof: true, batch: 46 },
  { id: '0xa8f2', type: 'BID', status: 'COMMITTED', proof: true, batch: 48 },
  { id: '0x3b7c', type: 'ASK', status: 'PROVING', proof: false, batch: null },
]

function StatusDot({ status }: { status: string }) {
  const color =
    status === 'SETTLED'
      ? 'bg-brand-accent'
      : status === 'MATCHED'
        ? 'bg-brand-accent/60'
        : 'bg-brand-border2'
  return (
    <span
      className={`inline-block w-1.5 h-1.5 ${color} ${status === 'PROVING' ? 'animate-blink' : ''}`}
      style={{ borderRadius: 0 }}
    />
  )
}

export default function OrderFlowPanel() {
  return (
    <div className="w-full h-full flex flex-col border border-brand-border bg-brand-bg/60 backdrop-blur-sm">
      {/* Panel header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-brand-border">
        <div className="flex items-center gap-2">
          <span
            className="w-1.5 h-1.5 bg-brand-accent animate-blink inline-block"
            style={{ borderRadius: 0 }}
          />
          <span className="font-mono text-[10px] text-brand-accent tracking-[0.15em]">
            ORDER FLOW
          </span>
        </div>
        <span className="font-mono text-[9px] text-brand-border2 tracking-[0.2em]">
          BATCH #47
        </span>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-[1fr_50px_80px_60px] px-4 py-2 border-b border-brand-border/50">
        {['ORDER', 'SIDE', 'STATUS', 'PROOF'].map((h) => (
          <span
            key={h}
            className="font-mono text-[8px] text-brand-muted tracking-[0.2em]"
          >
            {h}
          </span>
        ))}
      </div>

      {/* Order rows */}
      <div className="flex-1 overflow-hidden">
        {orders.map((order) => (
          <div
            key={order.id}
            className="grid grid-cols-[1fr_50px_80px_60px] px-4 py-2.5 border-b border-brand-border/20 hover:bg-brand-accent/[0.02] transition-colors"
          >
            <span className="font-mono text-[11px] text-brand-border2">
              {order.id}…
            </span>
            <span
              className={`font-mono text-[10px] ${
                order.type === 'BID' ? 'text-brand-accent/70' : 'text-brand-muted'
              }`}
            >
              {order.type}
            </span>
            <span className="font-mono text-[10px] text-brand-border2 flex items-center gap-1.5">
              <StatusDot status={order.status} />
              {order.status}
            </span>
            <span className="font-mono text-[10px] text-brand-accent/80">
              {order.proof ? '✓' : '···'}
            </span>
          </div>
        ))}
      </div>

      {/* Bottom bar */}
      <div className="px-4 py-3 border-t border-brand-border flex items-center justify-between">
        <div className="font-mono text-[9px] text-brand-border2 tracking-[0.1em]">
          <span className="text-brand-accent">████</span>
          <span> PRICE HIDDEN</span>
        </div>
        <div className="font-mono text-[9px] text-brand-border2 tracking-[0.1em]">
          <span className="text-brand-accent">████</span>
          <span> SIZE HIDDEN</span>
        </div>
        <div className="font-mono text-[9px] text-brand-border2 tracking-[0.1em]">
          256/256 BATCH
        </div>
      </div>
    </div>
  )
}
