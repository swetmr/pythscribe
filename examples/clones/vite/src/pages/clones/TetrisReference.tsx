import { Link } from 'react-router'
import { Tetris } from '../../../../shared/tetris/Tetris'

// Secondary reference — the React oracle, at /react-reference/tetris,
// mirroring the /tetris production route.
export default function TetrisReference() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Tetris />
    </div>
  )
}
