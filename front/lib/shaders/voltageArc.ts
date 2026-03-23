export function initVoltageArc(canvas: HTMLCanvasElement): () => void {
  const ctx = canvas.getContext('2d')!
  let raf: number
  let t = 0

  function resize() {
    const dpr = window.devicePixelRatio || 1
    canvas.width = canvas.offsetWidth * dpr
    canvas.height = canvas.offsetHeight * dpr
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  }

  resize()
  window.addEventListener('resize', resize)

  // Conductor nodes — they drift slowly
  const nodes = Array.from({ length: 6 }, () => ({
    x: Math.random(),
    y: Math.random(),
    vx: (Math.random() - 0.5) * 0.0003,
    vy: (Math.random() - 0.5) * 0.0003,
  }))

  function lightning(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    detail: number,
    displacement: number
  ): { x: number; y: number }[] {
    if (detail <= 1) return [{ x: x1, y: y1 }, { x: x2, y: y2 }]
    const midX = (x1 + x2) / 2 + (Math.random() - 0.5) * displacement
    const midY = (y1 + y2) / 2 + (Math.random() - 0.5) * displacement
    const left = lightning(x1, y1, midX, midY, detail - 1, displacement * 0.55)
    const right = lightning(midX, midY, x2, y2, detail - 1, displacement * 0.55)
    return [...left, ...right.slice(1)]
  }

  function draw() {
    const w = canvas.offsetWidth
    const h = canvas.offsetHeight

    // Fade previous frame
    ctx.save()
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.fillStyle = 'rgba(6,6,10,0.15)'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.restore()

    // Update node positions
    nodes.forEach((n) => {
      n.x += n.vx
      n.y += n.vy
      // Bounce at edges
      if (n.x < 0.05 || n.x > 0.95) n.vx *= -1
      if (n.y < 0.05 || n.y > 0.95) n.vy *= -1
      // Clamp
      n.x = Math.max(0.02, Math.min(0.98, n.x))
      n.y = Math.max(0.02, Math.min(0.98, n.y))
    })

    // Draw arcs between nearby nodes
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const a = nodes[i]
        const b = nodes[j]
        const dx = (a.x - b.x) * w
        const dy = (a.y - b.y) * h
        const dist = Math.sqrt(dx * dx + dy * dy)

        // Only arc between nearby nodes
        if (dist > w * 0.45) continue

        // Arc probability — closer = more frequent
        const prob = 1 - dist / (w * 0.45)
        if (Math.random() > prob * 0.15) continue

        const points = lightning(
          a.x * w,
          a.y * h,
          b.x * w,
          b.y * h,
          6,
          dist * 0.15
        )

        // Glow layer
        ctx.beginPath()
        ctx.strokeStyle = `rgba(212,255,0,${prob * 0.08})`
        ctx.lineWidth = 4
        points.forEach((p, idx) => {
          if (idx === 0) {
            ctx.moveTo(p.x, p.y)
          } else {
            ctx.lineTo(p.x, p.y)
          }
        })
        ctx.stroke()

        // Core arc
        ctx.beginPath()
        ctx.strokeStyle = `rgba(212,255,0,${prob * 0.25})`
        ctx.lineWidth = 1
        points.forEach((p, idx) => {
          if (idx === 0) {
            ctx.moveTo(p.x, p.y)
          } else {
            ctx.lineTo(p.x, p.y)
          }
        })
        ctx.stroke()
      }
    }

    // Draw conductor nodes — small glowing dots
    nodes.forEach((n) => {
      const pulse = Math.sin(t * 0.05 + n.x * 10) * 0.3 + 0.7
      // Outer glow
      ctx.beginPath()
      ctx.arc(n.x * w, n.y * h, 6, 0, Math.PI * 2)
      ctx.fillStyle = `rgba(212,255,0,${0.04 * pulse})`
      ctx.fill()
      // Core
      ctx.beginPath()
      ctx.arc(n.x * w, n.y * h, 2, 0, Math.PI * 2)
      ctx.fillStyle = `rgba(212,255,0,${0.3 * pulse})`
      ctx.fill()
    })

    t++
    raf = requestAnimationFrame(draw)
  }

  draw()

  return () => {
    cancelAnimationFrame(raf)
    window.removeEventListener('resize', resize)
  }
}
