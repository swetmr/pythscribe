/**
 * Kanban oracle page (React reference track).
 * Renders the KanbanBoard.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/kanban/page.ps.
 */
import { KanbanBoard } from '../../../../shared/kanban/KanbanBoard'

export default function Page() {
  return (
    <main style={{ padding: 20 }}>
      <p>
        <a href="/">&larr; home</a>
      </p>
      <KanbanBoard />
    </main>
  )
}
