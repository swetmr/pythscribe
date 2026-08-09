import Link from 'next/link'
import { CLONES } from '../../shared/clones'

export default function Home() {
  return (
    <div className="shell">
      <h1>pyths clone-demos</h1>
      <p style={{ color: 'var(--muted)' }}>
        Next.js 16 App Router shell — second production track. Shared
        components live under <code>shared/&lt;clone&gt;/</code> and mount
        identically here and in the Vite shell.
      </p>
      <h2>Scaffold proof</h2>
      <div className="clone-grid">
        <Link className="clone-card" href="/hello" data-testid="nav-hello">
          HelloCard demo
        </Link>
        <Link
          className="clone-card"
          href="/react-reference/hello"
          data-testid="nav-hello-reference"
        >
          HelloCard (React reference)
        </Link>
      </div>
      <h2>Clones</h2>
      <div className="clone-grid">
        {CLONES.map(({ slug, name, stretch }) => (
          <Link key={slug} className="clone-card" href={`/${slug}`} data-testid={`nav-${slug}`}>
            {name}
            {stretch ? ' (stretch)' : ''}
          </Link>
        ))}
      </div>
    </div>
  )
}
