import { Link } from 'react-router'
import { TwitterApp } from '../../../../shared/twitter/TwitterApp.psc'
import { twitterFixture } from '../../../../shared/twitter/fixtures'

// Production track — mounts the .ps island loaded LIVE by vite-plugin-pyths.
export default function TwitterPsc() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>Twitter (PythScribe, production)</h1>
      <TwitterApp {...twitterFixture} />
    </div>
  )
}
