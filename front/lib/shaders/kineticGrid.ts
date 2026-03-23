export function initKineticGrid(canvas: HTMLCanvasElement): () => void {
  const ctx = canvas.getContext('2d')!
  let raf: number
  let t = 0

  interface Node {
    baseX: number
    baseY: number
    x: number
    y: number
    offsetX: number
    offsetY: number
    phase: number
  }

  let nodes: Node[] = []
  let cols = 0
  let rows = 0
  const spacing = 48

  function resize() {
    const dpr = window.devicePixelRatio || 1
    canvas.width = canvas.offsetWidth * dpr
    canvas.height = canvas.offsetHeight * dpr
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const w = canvas.offsetWidth
    const h = canvas.offsetHeight
    cols = Math.ceil(w / spacing) + 2
    rows = Math.ceil(h / spacing) + 2

    nodes = []
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < cols; col++) {
        nodes.push({
          baseX: col * spacing - spacing / 2,
          baseY: row * spacing - spacing / 2,
          x: 0,
          y: 0,
          offsetX: 0,
          offsetY: 0,
          phase: Math.random() * Math.PI * 2,
        })
      }
    }
  }

  resize()
  window.addEventListener('resize', resize)

  function draw() {
    const w = canvas.offsetWidth
    const h = canvas.offsetHeight

    ctx.save()
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.fillStyle = 'rgba(6,6,10,0.25)'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.restore()

    // Traveling wave impulses
    const wave1X = (Math.sin(t * 0.008) * 0.5 + 0.5) * w
    const wave1Y = (Math.cos(t * 0.006) * 0.5 + 0.5) * h
    const wave2X = (Math.cos(t * 0.01 + 2) * 0.5 + 0.5) * w
    const wave2Y = (Math.sin(t * 0.007 + 1) * 0.5 + 0.5) * h

    // Update nodes
    nodes.forEach((n) => {
      const d1 = Math.sqrt((n.baseX - wave1X) ** 2 + (n.baseY - wave1Y) ** 2)
      const d2 = Math.sqrt((n.baseX - wave2X) ** 2 + (n.baseY - wave2Y) ** 2)

      const ripple1 = Math.sin(d1 * 0.02 - t * 0.06) * Math.max(0, 1 - d1 / (w * 0.6))
      const ripple2 = Math.sin(d2 * 0.025 - t * 0.05) * Math.max(0, 1 - d2 / (w * 0.5))

      const displacement = (ripple1 + ripple2) * 8
      const angle = Math.atan2(n.baseY - h / 2, n.baseX - w / 2) + n.phase

      n.offsetX = Math.cos(angle) * displacement
      n.offsetY = Math.sin(angle) * displacement
      n.x = n.baseX + n.offsetX
      n.y = n.baseY + n.offsetY
    })

    // Draw connections
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < cols; col++) {
        const idx = row * cols + col
        const n = nodes[idx]

        // Distance to nearest impulse center
        const d1 = Math.sqrt((n.x - wave1X) ** 2 + (n.y - wave1Y) ** 2)
        const d2 = Math.sqrt((n.x - wave2X) ** 2 + (n.y - wave2Y) ** 2)
        const nearestDist = Math.min(d1, d2)
        const proximity = Math.max(0, 1 - nearestDist / (w * 0.4))
        const baseAlpha = 0.03 + proximity * 0.1

        // Right neighbor
        if (col < cols - 1) {
          const right = nodes[idx + 1]
          ctx.beginPath()
          ctx.moveTo(n.x, n.y)
          ctx.lineTo(right.x, right.y)
          ctx.strokeStyle = `rgba(212,255,0,${baseAlpha})`
          ctx.lineWidth = 0.5 + proximity * 0.8
          ctx.stroke()
        }

        // Bottom neighbor
        if (row < rows - 1) {
          const below = nodes[idx + cols]
          ctx.beginPath()
          ctx.moveTo(n.x, n.y)
          ctx.lineTo(below.x, below.y)
          ctx.strokeStyle = `rgba(212,255,0,${baseAlpha})`
          ctx.lineWidth = 0.5 + proximity * 0.8
          ctx.stroke()
        }

        // Node dot
        const dotAlpha = 0.05 + proximity * 0.35
        const dotSize = 1 + proximity * 1.5
        ctx.fillStyle = `rgba(212,255,0,${dotAlpha})`
        ctx.fillRect(n.x - dotSize / 2, n.y - dotSize / 2, dotSize, dotSize)
      }
    }

    t++
    raf = requestAnimationFrame(draw)
  }

  draw()

  return () => {
    cancelAnimationFrame(raf)
    window.removeEventListener('resize', resize)
  }
}
