import { Link } from 'react-router'
import { Tetris } from '../../../../shared/tetris/Tetris.psc'

// Production track — mounts the shared/tetris .psc island loaded LIVE by
// vite-plugin-pyths (no precompile step, unlike the Next shell).
export default function TetrisPsc() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Tetris />
    </div>
  )
}
