'use client'

import { useRef, useEffect } from 'react'
import { useGSAP } from '@gsap/react'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { initSparkChamber } from '@/lib/shaders/sparkChamber'

gsap.registerPlugin(useGSAP, ScrollTrigger)

const steps = [
  {
    num: '01',
    title: 'COMMIT',
    body: 'Pedersen commitment generated locally. Price, pair and size never leave the device.',
    tech: 'PEDERSEN COMMITMENT',
  },
  {
    num: '02',
    title: 'PROVE',
    body: 'Rust circuit generates a ZK proof of order validity — collateral, format, limits.',
    tech: 'halo2 / RUST WASM',
  },
  {
    num: '03',
    title: 'MATCH',
    body: 'Engine receives only commitments and validity bits. Never sees real order data.',
    tech: 'GO MATCHING ENGINE',
  },
  {
    num: '04',
    title: 'SETTLE',
    body: 'Batch of 256 matched pairs with aggregated proof verified on-chain. Atomic release.',
    tech: 'SOLIDITY + EVM',
  },
]

const guarantees = [
  { value: 'ZERO', label: 'DATA LEAKED', sub: 'Private by default' },
  { value: 'NONE', label: 'MEV EXPOSURE', sub: 'No visible mempool' },
  { value: '256', label: 'TRADES / TX', sub: 'Aggregated proofs' },
]

export default function HowItWorks() {
  const containerRef = useRef<HTMLElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    if (!canvasRef.current) return
    return initSparkChamber(canvasRef.current)
  }, [])

  useGSAP(
    () => {
      gsap.from('.how-label', {
        scrollTrigger: { trigger: '.how-header', start: 'top 80%' },
        y: 20,
        opacity: 0,
        duration: 0.6,
        ease: 'power3.out',
      })

      gsap.from('.how-title', {
        scrollTrigger: { trigger: '.how-header', start: 'top 80%' },
        y: 40,
        opacity: 0,
        duration: 0.8,
        delay: 0.1,
        ease: 'power3.out',
      })

      gsap.from('.how-desc', {
        scrollTrigger: { trigger: '.how-header', start: 'top 80%' },
        y: 20,
        opacity: 0,
        duration: 0.6,
        delay: 0.25,
        ease: 'power2.out',
      })

      gsap.from('.step-row', {
        scrollTrigger: { trigger: '.steps-list', start: 'top 78%' },
        x: 30,
        opacity: 0,
        duration: 0.6,
        stagger: 0.1,
        ease: 'power3.out',
      })

      gsap.from('.guarantee-item', {
        scrollTrigger: { trigger: '.guarantees-bar', start: 'top 85%' },
        y: 20,
        opacity: 0,
        duration: 0.5,
        stagger: 0.08,
        ease: 'power2.out',
      })
    },
    { scope: containerRef }
  )

  return (
    <section
      ref={containerRef}
      className="relative bg-brand-bg overflow-hidden border-t border-brand-border"
    >
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full opacity-[0.12] pointer-events-none z-0"
      />

      <div className="relative z-10 px-6 md:px-12 lg:px-20 py-24 md:py-32 md:pr-[300px]">
        {/* Two-column: title left, steps right */}
        <div className="grid grid-cols-1 lg:grid-cols-[1fr_1fr] gap-16 lg:gap-24 mb-24">
          {/* LEFT — header */}
          <div className="how-header lg:sticky lg:top-32 lg:self-start">
            <span className="how-label font-mono text-[10px] text-brand-accent tracking-[0.3em] block mb-6">
              HOW IT WORKS
            </span>

            <h2 className="how-title font-display text-[clamp(40px,5vw,72px)] leading-[0.92] text-white mb-8">
              FOUR STEPS,
              <br />
              <span className="text-brand-accent">ZERO KNOWLEDGE.</span>
            </h2>

            <p className="how-desc font-mono text-[12px] text-brand-muted leading-[1.8] max-w-[380px]">
              Each step is cryptographically isolated. The protocol never has
              enough information to reconstruct your order. Not even the
              operator knows what you&apos;re trading.
            </p>
          </div>

          {/* RIGHT — steps timeline */}
          <div className="steps-list relative">
            {/* Vertical line */}
            <div className="absolute left-[15px] top-4 bottom-4 w-px bg-brand-border hidden lg:block" />

            {steps.map((step, i) => (
              <div
                key={step.num}
                className={`step-row flex gap-6 lg:gap-8 ${
                  i < steps.length - 1 ? 'mb-8 lg:mb-10' : ''
                }`}
              >
                {/* Number node */}
                <div className="relative flex-shrink-0 w-[30px] flex flex-col items-center">
                  <div className="w-[30px] h-[30px] border border-brand-border bg-brand-bg flex items-center justify-center z-10">
                    <span className="font-mono text-[10px] text-brand-accent">
                      {step.num}
                    </span>
                  </div>
                </div>

                {/* Content */}
                <div className="flex-1 border border-brand-border p-5 bg-brand-bg/40 group hover:border-brand-accent/30 transition-colors duration-300">
                  <div className="flex items-baseline justify-between mb-3">
                    <h3 className="font-display text-[24px] text-white">
                      {step.title}
                    </h3>
                    <span className="font-mono text-[8px] text-brand-muted tracking-[0.2em]">
                      {step.tech}
                    </span>
                  </div>
                  <p className="font-mono text-[11px] text-brand-muted leading-[1.75]">
                    {step.body}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* GUARANTEES BAR */}
        <div className="guarantees-bar border-t border-brand-border pt-10">
          <span className="font-mono text-[10px] text-brand-muted tracking-[0.3em] block mb-8">
            PROTOCOL GUARANTEES
          </span>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-8 md:gap-0">
            {guarantees.map((g, i) => (
              <div
                key={g.label}
                className={`guarantee-item ${
                  i < guarantees.length - 1 ? 'md:border-r md:border-brand-border' : ''
                } md:pr-8 ${i > 0 ? 'md:pl-8' : ''}`}
              >
                <div className="font-display text-[clamp(36px,4vw,56px)] text-brand-accent leading-none">
                  {g.value}
                </div>
                <div className="font-mono text-[9px] text-brand-muted tracking-[0.2em] mt-2">
                  {g.label}
                </div>
                <div className="font-mono text-[11px] text-brand-border2 mt-1">
                  {g.sub}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  )
}
