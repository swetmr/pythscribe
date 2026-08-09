import { Link } from 'react-router'
import { KanbanBoard } from '../../../../shared/kanban/KanbanBoard.ps'

// Production track — mounts the shared/kanban .ps island loaded LIVE by
// vite-plugin-pyths (no precompile step, unlike the Next shell).
export default function Kanban() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <KanbanBoard />
    </div>
  )
}
