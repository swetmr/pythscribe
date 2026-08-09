import { Link } from 'react-router'
import { KanbanBoard } from '../../../../shared/kanban/KanbanBoard'

// Secondary reference — the React oracle, at /react-reference/kanban,
// mirroring the /kanban production route.
export default function KanbanReference() {
  return (
    <div className="shell" style={{ maxWidth: 1200 }}>
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <KanbanBoard />
    </div>
  )
}
