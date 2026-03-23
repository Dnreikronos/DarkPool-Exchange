export function initGridPulse(canvas: HTMLCanvasElement): () => void {
  const ctx = canvas.getContext('2d')!
  let raf: number
  let t = 0

  function resize() {
    canvas.width = canvas.offsetWidth * window.devicePixelRatio
    canvas.height = canvas.offsetHeight * window.devicePixelRatio
    ctx.scale(window.devicePixelRatio, window.devicePixelRatio)
  }

  resize()
  window.addEventListener('resize', resize)

  function draw() {
    const w = canvas.width / window.devicePixelRatio
    const h = canvas.height / window.devicePixelRatio

    ctx.save()
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.restore()

    const step = 40
    const cols = Math.ceil(w / step) + 1
    const rows = Math.ceil(h / step) + 1

    // Pulsing grid nodes
    for (let col = 0; col < cols; col++) {
      for (let row = 0; row < rows; row++) {
        const x = col * step
        const y = row * step

        // Wave propagation from multiple centers
        const dist1 = Math.sqrt((x - w * 0.3) ** 2 + (y - h * 0.4) ** 2)
        const dist2 = Math.sqrt((x - w * 0.7) ** 2 + (y - h * 0.6) ** 2)

        const wave1 = Math.sin(dist1 * 0.015 - t * 0.04) * 0.5 + 0.5
        const wave2 = Math.sin(dist2 * 0.012 - t * 0.03 + 1.5) * 0.5 + 0.5
        const intensity = (wave1 + wave2) * 0.5

        const alpha = intensity * 0.35 + 0.02
        const size = intensity * 2.5 + 0.5

        ctx.fillStyle = `rgba(212,255,0,${alpha})`
        ctx.fillRect(x - size / 2, y - size / 2, size, size)
      }
    }

    // Horizontal scan lines that drift
    for (let i = 0; i < 3; i++) {
      const scanY = ((t * 0.5 + i * h / 3) % (h + 40)) - 20
      const gradient = ctx.createLinearGradient(0, scanY - 15, 0, scanY + 15)
      gradient.addColorStop(0, 'rgba(212,255,0,0)')
      gradient.addColorStop(0.5, 'rgba(212,255,0,0.06)')
      gradient.addColorStop(1, 'rgba(212,255,0,0)')
      ctx.fillStyle = gradient
      ctx.fillRect(0, scanY - 15, w, 30)
    }

    // Connecting lines between bright nodes
    ctx.strokeStyle = 'rgba(212,255,0,0.04)'
    ctx.lineWidth = 0.5
    for (let col = 0; col < cols - 1; col++) {
      for (let row = 0; row < rows - 1; row++) {
        const x = col * step
        const y = row * step
        const dist = Math.sqrt((x - w * 0.5) ** 2 + (y - h * 0.5) ** 2)
        const wave = Math.sin(dist * 0.01 - t * 0.03) * 0.5 + 0.5
        if (wave > 0.6) {
          ctx.beginPath()
          ctx.moveTo(x, y)
          ctx.lineTo(x + step, y)
          ctx.stroke()
          ctx.beginPath()
          ctx.moveTo(x, y)
          ctx.lineTo(x, y + step)
          ctx.stroke()
        }
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
