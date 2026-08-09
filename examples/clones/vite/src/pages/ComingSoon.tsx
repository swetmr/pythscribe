import { Link } from 'react-router'

// Placeholder for an as-yet-unbuilt clone route. Scaffold deliverable #1
// only requires the landing page to LINK the clone routes — no clone
// content ships in this scaffold.
export default function ComingSoon({ name, oracle = false }: { name: string; oracle?: boolean }) {
  return (
    <div className="shell" data-testid="coming-soon">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>
        {name} {oracle ? '(React reference)' : ''}
      </h1>
      <p style={{ color: 'var(--muted)' }}>Coming soon.</p>
    </div>
  )
}
