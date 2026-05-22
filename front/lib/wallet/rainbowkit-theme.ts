import { darkTheme, type Theme } from '@rainbow-me/rainbowkit'

/**
 * DarkPool brutalist theme for the RainbowKit modal.
 *
 * DESIGN.md tokens we map:
 *   - accent       → #D4FF00 (single neon-lime accent)
 *   - bg           → #06060A surface
 *   - border       → 0 radius everywhere
 *   - typography   → IBM Plex Mono for body, Bebas Neue handled at the
 *     surrounding ConnectButton level (RainbowKit's modal still uses
 *     its own font stack inside; that's acceptable scope).
 */
export const darkpoolRainbowTheme: Theme = darkTheme({
  accentColor: '#D4FF00',
  accentColorForeground: '#06060A',
  borderRadius: 'none',
  fontStack: 'system',
  overlayBlur: 'small',
})
