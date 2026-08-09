/**
 * Twitter clone oracle page (React reference track).
 * Renders the TwitterApp.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/twitter/page.ps.
 */
import { TwitterApp } from '../../../../shared/twitter/TwitterApp'
import { twitterFixture } from '../../../../shared/twitter/fixtures'

export default function Page() {
  return (
    <main style={{ padding: 20 }}>
      <p>
        <a href="/">&larr; home</a>
      </p>
      <h1>Twitter (React reference)</h1>
      <TwitterApp {...twitterFixture} />
    </main>
  )
}
