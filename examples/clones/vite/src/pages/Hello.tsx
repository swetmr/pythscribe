import { Link } from 'react-router'
import { HelloCard } from '../../../shared/hello/HelloCard.ps'
import { helloFixture } from '../../../shared/hello/fixtures'

// Production track — mounts the .ps island loaded LIVE by vite-plugin-pyths
// (importer-aware resolve; no precompile step, unlike the Next shell).
export default function Hello() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>HelloCard (PythScribe, production)</h1>
      <HelloCard {...helloFixture} />
    </div>
  )
}
