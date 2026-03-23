'use client'

const iterations = [
  { hex: '0xc3f1a7', batch: 'batch_47' },
  { hex: '0x9e2b4d', batch: 'batch_48' },
  { hex: '0x71fa83', batch: 'batch_49' },
  { hex: '0xd4e09c', batch: 'batch_50' },
  { hex: '0xa8f2e1', batch: 'batch_51' },
  { hex: '0x3b7c9f', batch: 'batch_52' },
  { hex: '0xef41d6', batch: 'batch_53' },
  { hex: '0x62ca08', batch: 'batch_54' },
]

function TerminalBlock() {
  return (
    <>
      {iterations.map((item, i) => (
        <div key={i} className="py-1.5">
          <div>{item.hex}… COMMIT</div>
          <div>
            <span className="text-brand-accent">PROOF ✓</span>
            <span>&nbsp;&nbsp;[{item.batch}]</span>
          </div>
          <div>████████████ HIDDEN</div>
          <div>ASK&nbsp;&nbsp;░░░░░&nbsp;&nbsp;HIDDEN</div>
          <div>BID&nbsp;&nbsp;░░░░░&nbsp;&nbsp;HIDDEN</div>
          <div>MATCHED → SETTLE</div>
          <div className="select-none">─────────────────</div>
        </div>
      ))}
    </>
  )
}

export default function TerminalFeed() {
  return (
    <div className="fixed top-0 right-0 bottom-0 w-[260px] overflow-hidden pointer-events-none z-[1] hidden md:block">
      <div className="animate-terminal-scroll font-mono text-[10px] leading-relaxed text-brand-border2 opacity-40 pr-5 pl-4 pt-4 text-right">
        <TerminalBlock />
        <TerminalBlock />
      </div>
    </div>
  )
}
