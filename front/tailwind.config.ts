import type { Config } from 'tailwindcss'

const config: Config = {
  content: [
    './pages/**/*.{js,ts,jsx,tsx,mdx}',
    './components/**/*.{js,ts,jsx,tsx,mdx}',
    './app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
    borderRadius: {
      DEFAULT: '0',
      none: '0',
      sm: '0',
      md: '0',
      lg: '0',
      xl: '0',
      '2xl': '0',
      '3xl': '0',
      full: '0',
    },
    extend: {
      colors: {
        brand: {
          bg: '#06060A',
          surface: '#0C0C12',
          border: '#1C1C26',
          border2: '#2E2E3E',
          muted: '#5A5A72',
          accent: '#D4FF00',
          'on-accent': '#06060A',
          fg: '#FFFFFF',
        },
      },
      fontFamily: {
        display: ['var(--font-bebas)'],
        mono: ['var(--font-ibm-plex-mono)'],
      },
      fontSize: {
        'display-xl': ['148px', { lineHeight: '0.88', letterSpacing: '0', fontWeight: '400' }],
        'display-lg': ['72px', { lineHeight: '0.92', letterSpacing: '0', fontWeight: '400' }],
        'display-md': ['48px', { lineHeight: '0.95', letterSpacing: '0', fontWeight: '400' }],
        'display-sm': ['24px', { lineHeight: '1', letterSpacing: '0', fontWeight: '400' }],
        'headline-md': [
          '20px',
          { lineHeight: '1', letterSpacing: '0.05em', fontWeight: '400' },
        ],
        'body-lg': ['14px', { lineHeight: '1.85', fontWeight: '400' }],
        'body-md': ['12px', { lineHeight: '1.8', fontWeight: '400' }],
        'body-sm': ['11px', { lineHeight: '1.75', fontWeight: '400' }],
        'label-lg': [
          '11px',
          { lineHeight: '1.4', letterSpacing: '0.15em', fontWeight: '500' },
        ],
        'label-md': [
          '10px',
          { lineHeight: '1.4', letterSpacing: '0.2em', fontWeight: '500' },
        ],
        'label-sm': [
          '8px',
          { lineHeight: '1.4', letterSpacing: '0.2em', fontWeight: '500' },
        ],
      },
      letterSpacing: {
        label: '0.15em',
        labelWide: '0.2em',
        brand: '0.05em',
      },
      spacing: {
        'page-x-mobile': '24px',
        'page-x-tablet': '48px',
        'page-x-desktop': '80px',
        'gutter-right': '280px',
      },
      boxShadow: {
        'accent-glow': '0 0 32px rgba(212, 255, 0, 0.45)',
      },
      keyframes: {
        marquee: {
          '0%': { transform: 'translateX(0)' },
          '100%': { transform: 'translateX(-50%)' },
        },
        terminalScroll: {
          '0%': { transform: 'translateY(0)' },
          '100%': { transform: 'translateY(-50%)' },
        },
        blink: {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0' },
        },
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'fade-out': {
          '0%': { opacity: '1' },
          '100%': { opacity: '0' },
        },
        'slide-in-right': {
          '0%': { transform: 'translateX(100%)' },
          '100%': { transform: 'translateX(0)' },
        },
        'slide-out-right': {
          '0%': { transform: 'translateX(0)' },
          '100%': { transform: 'translateX(100%)' },
        },
      },
      animation: {
        marquee: 'marquee 30s linear infinite',
        'terminal-scroll': 'terminalScroll 22s linear infinite',
        blink: 'blink 1s ease-in-out infinite',
        'fade-in': 'fade-in 150ms ease-out',
        'fade-out': 'fade-out 150ms ease-in',
        'slide-in-right': 'slide-in-right 220ms ease-out',
        'slide-out-right': 'slide-out-right 220ms ease-in',
      },
    },
  },
  plugins: [],
}
export default config
