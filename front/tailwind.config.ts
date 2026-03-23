import type { Config } from "tailwindcss";

const config: Config = {
  content: [
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    borderRadius: {},
    extend: {
      colors: {
        brand: {
          bg: '#06060A',
          surface: '#0C0C12',
          border: '#1C1C26',
          border2: '#2E2E3E',
          muted: '#5A5A72',
          accent: '#D4FF00',
        },
      },
      fontFamily: {
        display: ['var(--font-bebas)'],
        mono: ['var(--font-ibm-plex-mono)'],
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
      },
      animation: {
        marquee: 'marquee 30s linear infinite',
        'terminal-scroll': 'terminalScroll 22s linear infinite',
        blink: 'blink 1s ease-in-out infinite',
      },
    },
  },
  plugins: [],
};
export default config;
