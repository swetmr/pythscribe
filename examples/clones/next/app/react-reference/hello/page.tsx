/**
 * HelloCard oracle page (React reference track).
 * Renders the HelloCard.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/hello/page.ps.
 */
import { HelloCard } from '../../../../shared/hello/HelloCard'
import { helloFixture } from '../../../../shared/hello/fixtures'

export default function Page() {
  return (
    <main style={{ padding: 20 }}>
      <p>
        <a href="/">&larr; home</a>
      </p>
      <h1>HelloCard (React reference)</h1>
      <HelloCard {...helloFixture} />
    </main>
  )
}
