import { Link } from 'react-router'
import { Tetris } from '../../../../shared/tetris/Tetris.ps'

// Production track — mounts the shared/tetris .ps island loaded LIVE by
// vite-plugin-pyths (no precompile step, unlike the Next shell).
export default function TetrisPage() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Tetris />
    </div>
  )
}
