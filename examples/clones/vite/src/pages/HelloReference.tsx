import { Link } from 'react-router'
import { HelloCard } from '../../../shared/hello/HelloCard'
import { helloFixture } from '../../../shared/hello/fixtures'

// Secondary reference — the React oracle, at /react-reference/hello,
// mirroring the /hello production route.
export default function HelloReference() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>HelloCard (React reference)</h1>
      <HelloCard {...helloFixture} />
    </div>
  )
}
