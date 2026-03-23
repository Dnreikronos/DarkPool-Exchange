import type { Metadata } from 'next'
import { Bebas_Neue, IBM_Plex_Mono } from 'next/font/google'
import './globals.css'

const bebas = Bebas_Neue({
  weight: '400',
  subsets: ['latin'],
  variable: '--font-bebas',
  display: 'swap',
})

const ibmPlexMono = IBM_Plex_Mono({
  weight: ['400', '500'],
  subsets: ['latin'],
  variable: '--font-ibm-plex-mono',
  display: 'swap',
})

export const metadata: Metadata = {
  title: 'DarkPool — Trade Without Revealing Anything',
  description:
    'A decentralized exchange where orders stay private until settlement. ZK-SNARK verified, MEV resistant, gas efficient.',
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang="en">
      <body
        className={`${bebas.variable} ${ibmPlexMono.variable} antialiased bg-brand-bg`}
      >
        {children}
      </body>
    </html>
  )
}
