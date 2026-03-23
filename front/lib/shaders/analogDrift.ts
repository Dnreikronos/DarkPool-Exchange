export function initAnalogDrift(canvas: HTMLCanvasElement): () => void {
  const ctx = canvas.getContext('2d')!
  let raf: number
  let t = 0

  function resize() {
    const dpr = window.devicePixelRatio || 1
    canvas.width = canvas.offsetWidth * dpr
    canvas.height = canvas.offsetHeight * dpr
    // Reset transform completely before re-scaling
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  }

  resize()
  window.addEventListener('resize', resize)

  function drawGrid() {
    const w = canvas.offsetWidth
    const h = canvas.offsetHeight
    ctx.strokeStyle = 'rgba(212,255,0,0.04)'
    ctx.lineWidth = 0.5
    const step = 60
    for (let x = 0; x < w; x += step) {
      ctx.beginPath()
      ctx.moveTo(x, 0)
      ctx.lineTo(x, h)
      ctx.stroke()
    }
    for (let y = 0; y < h; y += step) {
      ctx.beginPath()
      ctx.moveTo(0, y)
      ctx.lineTo(w, y)
      ctx.stroke()
    }
  }

  function draw() {
    const w = canvas.offsetWidth
    const h = canvas.offsetHeight

    // Phosphor persistence
    ctx.save()
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.fillStyle = 'rgba(6,6,10,0.12)'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.restore()

    drawGrid()

    const cx = w / 2
    const cy = h / 2
    // Use the smaller dimension so figures never stretch
    const radius = Math.min(w, h) * 0.5

    const figures = [
      { A: radius * 0.7, B: radius * 0.7, a: 3, b: 2, delta: t * 0.003 },
      { A: radius * 0.5, B: radius * 0.5, a: 5, b: 4, delta: t * 0.002 + 1 },
      { A: radius * 0.35, B: radius * 0.35, a: 2, b: 3, delta: t * 0.0015 + 2 },
    ]

    figures.forEach(({ A, B, a, b, delta }) => {
      ctx.beginPath()
      ctx.strokeStyle = 'rgba(212,255,0,0.16)'
      ctx.lineWidth = 1.2
      for (let i = 0; i <= 628; i++) {
        const p = (i / 628) * Math.PI * 2
        const x = cx + A * Math.sin(a * p + delta)
        const y = cy + B * Math.sin(b * p)
        if (i === 0) {
          ctx.moveTo(x, y)
        } else {
          ctx.lineTo(x, y)
        }
      }
      ctx.stroke()
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
