import { Link } from 'react-router'
import { Game2048 } from '../../../../shared/2048/Game2048.psc'

// Production track — mounts the shared/2048 .psc island loaded LIVE by
// vite-plugin-pyths (no precompile step, unlike the Next shell).
export default function Game2048Psc() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <Game2048 />
    </div>
  )
}
