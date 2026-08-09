import { Link } from 'react-router'
import { Game2048 } from '../../../../shared/2048/Game2048.ps'

// Production track — mounts the shared/2048 .ps island loaded LIVE by
// vite-plugin-pyths (no precompile step, unlike the Next shell).
export default function Game2048Page() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Game2048 />
    </div>
  )
}
