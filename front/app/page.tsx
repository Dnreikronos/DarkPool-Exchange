import Nav from '@/components/Nav'
import Hero from '@/components/Hero'
import HowItWorks from '@/components/HowItWorks'
import Footer from '@/components/Footer'
import TerminalFeed from '@/components/TerminalFeed'

export default function Home() {
  return (
    <main className="relative">
      <TerminalFeed />
      <Nav />
      <Hero />
      <HowItWorks />
      <Footer />
    </main>
  )
}
