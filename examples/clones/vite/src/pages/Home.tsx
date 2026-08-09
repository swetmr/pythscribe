import { Link } from 'react-router'
import { CLONES } from '../../../shared/clones'

export default function Home() {
  return (
    <div className="shell">
      <h1>pyths clone-demos</h1>
      <p style={{ color: 'var(--muted)' }}>
        React (Vite) shell — production track. Shared components live under{' '}
        <code>shared/&lt;clone&gt;/</code> and mount identically here and in the Next.js
        shell.
      </p>
      <h2>Scaffold proof</h2>
      <div className="clone-grid">
        <Link className="clone-card" to="/hello" data-testid="nav-hello">
          HelloCard demo
        </Link>
        <Link className="clone-card" to="/react-reference/hello" data-testid="nav-hello-reference">
          HelloCard (React reference)
        </Link>
      </div>
      <h2>Clones</h2>
      <div className="clone-grid">
        {CLONES.map(({ slug, name, stretch }) => (
          <Link key={slug} className="clone-card" to={`/${slug}`} data-testid={`nav-${slug}`}>
            {name}
            {stretch ? ' (stretch)' : ''}
          </Link>
        ))}
      </div>
      <h2>Compressed clones (.psc)</h2>
      <p style={{ color: 'var(--muted)' }}>
        The same components authored in the compressed <code>.psc</code> layer — expanded
        to canonical <code>.ps</code> and compiled on import. Behavior is identical to the
        clones above; only the tracked source form differs.
      </p>
      <div className="clone-grid">
        {CLONES.map(({ slug, name, stretch }) => (
          <Link
            key={slug}
            className="clone-card"
            to={`/${slug}-psc`}
            data-testid={`nav-${slug}-psc`}
          >
            {name} (.psc)
            {stretch ? ' (stretch)' : ''}
          </Link>
        ))}
        <Link className="clone-card" to="/hello-psc" data-testid="nav-hello-psc">
          HelloCard (.psc)
        </Link>
      </div>
    </div>
  )
}
