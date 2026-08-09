import { Link } from 'react-router'
import { TwitterApp } from '../../../../shared/twitter/TwitterApp'
import { twitterFixture } from '../../../../shared/twitter/fixtures'

// Secondary reference — the React oracle, at /react-reference/twitter,
// mirroring the /twitter production route.
export default function TwitterReference() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>Twitter (React reference)</h1>
      <TwitterApp {...twitterFixture} />
    </div>
  )
}
