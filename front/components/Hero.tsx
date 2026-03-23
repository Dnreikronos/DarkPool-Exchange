'use client'

import { useRef, useEffect } from 'react'
import { useGSAP } from '@gsap/react'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { initKineticGrid } from '@/lib/shaders/kineticGrid'
import Ticker from './Ticker'

gsap.registerPlugin(useGSAP, ScrollTrigger)

const stats = [
  { value: '100,000', label: 'ORDERS/SEC', accent: true },
  { value: '< 1ms', label: 'P99 LATENCY', accent: false },
  { value: '256', label: 'ORDERS/BATCH', accent: false },
  { value: '0.05%', label: 'PROTOCOL FEE', accent: false },
]

export default function Hero() {
  const containerRef = useRef<HTMLElement>(null)
  const bgCanvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    if (!bgCanvasRef.current) return
    return initKineticGrid(bgCanvasRef.current)
  }, [])

  useGSAP(
    () => {
      const tl = gsap.timeline()
      tl.from('.hero-tag', {
        y: 20,
        opacity: 0,
        duration: 0.7,
        ease: 'power3.out',
      })
      tl.from(
        '.hero-h1 .line-reveal',
        {
          y: 60,
          opacity: 0,
          duration: 0.8,
          stagger: 0.1,
          ease: 'power4.out',
        },
        '-=0.4'
      )
      tl.from(
        '.hero-sub',
        { y: 16, opacity: 0, duration: 0.6, ease: 'power2.out' },
        '-=0.3'
      )
      tl.from(
        '.hero-buttons',
        { y: 16, opacity: 0, duration: 0.6, ease: 'power2.out' },
        '-=0.3'
      )
      tl.from(
        '.stat-item',
        {
          y: 12,
          opacity: 0,
          duration: 0.5,
          stagger: 0.07,
          ease: 'power2.out',
        },
        '-=0.5'
      )
    },
    { scope: containerRef }
  )

  return (
    <section
      ref={containerRef}
      className="relative h-screen flex flex-col overflow-hidden bg-brand-bg"
    >
      <canvas
        ref={bgCanvasRef}
        className="absolute inset-0 w-full h-full z-0"
      />

      {/* Ticker */}
      <div className="relative z-10 pt-20">
        <Ticker />
      </div>

      {/* Two-column content */}
      <div className="relative z-10 flex-1 flex items-center px-6 md:px-12 lg:px-20 md:pr-[300px]">
        <div className="w-full">
        {/* LEFT — copy */}
        <div>
          <p className="hero-tag font-mono text-[11px] text-brand-accent tracking-[0.2em] mb-6">
            [ PROTOCOL v0.1 — ARBITRUM ]
          </p>

          <h1 className="hero-h1 font-display leading-[0.90] mb-8">
            <div style={{ overflow: 'hidden' }}>
              <span className="line-reveal block text-white text-[clamp(52px,7.5vw,96px)]">
                TRADE WITHOUT
              </span>
            </div>
            <div style={{ overflow: 'hidden' }}>
              <span
                className="line-reveal block text-transparent text-[clamp(52px,7.5vw,96px)]"
                style={{ WebkitTextStroke: '2px #D4FF00' }}
              >
                REVEALING
              </span>
            </div>
            <div style={{ overflow: 'hidden' }}>
              <span className="line-reveal block text-white text-[clamp(52px,7.5vw,96px)]">
                ANYTHING.
              </span>
            </div>
          </h1>

          <p className="hero-sub font-mono text-[13px] text-brand-muted leading-[1.85] max-w-[420px] mb-8">
            Orders are cryptographic commitments.
            <br />
            The engine never sees price, pair or size.
            <br />
            Settlement is verified. Nothing is revealed.
          </p>

          <div className="hero-buttons flex gap-4">
            <button className="bg-brand-accent text-brand-bg font-mono text-xs font-medium px-8 py-4 transition-shadow duration-300 hover:shadow-[0_0_32px_rgba(212,255,0,0.45)]">
              ENTER APP
            </button>
            <button className="border border-brand-border2 text-brand-muted font-mono text-xs px-8 py-4 hover:border-brand-muted hover:text-white transition-colors">
              READ DOCS
            </button>
          </div>
        </div>

        </div>
      </div>

      {/* Bottom stats */}
      <div className="relative z-10 border-t border-brand-border flex-shrink-0">
        <div className="grid grid-cols-2 gap-4 md:flex md:gap-16 px-6 md:px-12 lg:px-20 py-6">
          {stats.map((stat) => (
            <div key={stat.label} className="stat-item">
              <div
                className={`font-display text-[38px] ${
                  stat.accent ? 'text-brand-accent' : 'text-white'
                }`}
              >
                {stat.value}
              </div>
              <div className="font-mono text-[10px] text-brand-muted tracking-[0.15em] uppercase mt-1">
                {stat.label}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
