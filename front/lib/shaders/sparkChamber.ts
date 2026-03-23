export function initSparkChamber(canvas: HTMLCanvasElement): () => void {
  const ctx = canvas.getContext('2d')!
  let raf: number

  const B = 0.018
  const dt = 0.6

  interface Particle {
    x: number
    y: number
    vx: number
    vy: number
    q: number
    alpha: number
    trail: { x: number; y: number }[]
  }

  let particles: Particle[] = []

  function resize() {
    const dpr = window.devicePixelRatio || 1
    canvas.width = canvas.offsetWidth * dpr
    canvas.height = canvas.offsetHeight * dpr
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const w = canvas.offsetWidth
    const h = canvas.offsetHeight

    if (particles.length === 0) {
      particles = Array.from({ length: 100 }, () => ({
        x: Math.random() * w,
        y: Math.random() * h,
        vx: (Math.random() - 0.5) * 2.5,
        vy: (Math.random() - 0.5) * 2.5,
        q: Math.random() > 0.5 ? 1 : -1,
        alpha: Math.random() * 0.4 + 0.1,
        trail: [],
      }))
    }
  }

  resize()
  window.addEventListener('resize', resize)

  function draw() {
    const w = canvas.offsetWidth
    const h = canvas.offsetHeight

    ctx.save()
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.fillStyle = 'rgba(12,12,18,0.1)'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.restore()

    particles.forEach((p) => {
      const ax = p.q * p.vy * B
      const ay = -p.q * p.vx * B
      p.vx += ax * dt
      p.vy += ay * dt

      const speed = Math.sqrt(p.vx ** 2 + p.vy ** 2)
      if (speed > 3) {
        p.vx = (p.vx / speed) * 3
        p.vy = (p.vy / speed) * 3
      }

      p.x += p.vx * dt
      p.y += p.vy * dt

      if (p.x < 0) p.x = w
      if (p.x > w) p.x = 0
      if (p.y < 0) p.y = h
      if (p.y > h) p.y = 0

      p.trail.push({ x: p.x, y: p.y })
      if (p.trail.length > 16) p.trail.shift()

      // Draw trail
      for (let i = 0; i < p.trail.length - 1; i++) {
        const a = (i / p.trail.length) * p.alpha * 0.3
        ctx.beginPath()
        ctx.moveTo(p.trail[i].x, p.trail[i].y)
        ctx.lineTo(p.trail[i + 1].x, p.trail[i + 1].y)
        ctx.strokeStyle = `rgba(212,255,0,${a})`
        ctx.lineWidth = 0.6
        ctx.stroke()
      }

      // Particle dot
      ctx.beginPath()
      ctx.arc(p.x, p.y, 1.2, 0, Math.PI * 2)
      ctx.fillStyle = `rgba(212,255,0,${p.alpha})`
      ctx.fill()
    })

    raf = requestAnimationFrame(draw)
  }

  draw()
  return () => {
    cancelAnimationFrame(raf)
    window.removeEventListener('resize', resize)
  }
}
