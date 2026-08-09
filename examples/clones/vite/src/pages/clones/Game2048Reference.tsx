import { Link } from 'react-router'
import { Game2048 } from '../../../../shared/2048/Game2048'

// Secondary reference — the React oracle, at /react-reference/2048,
// mirroring the /2048 production route.
export default function Game2048Reference() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Game2048 />
    </div>
  )
}
