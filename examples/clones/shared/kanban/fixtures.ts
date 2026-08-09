// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). The Kanban board's seed state: 3 columns, 6 cards.
// IDs follow the "c<N>" / "col<N>" convention that KanbanBoard's
// next-id helpers parse — keep new fixture entries on the same pattern.

export interface KanbanCard {
  id: string
  text: string
}

export interface KanbanColumn {
  id: string
  title: string
  cards: KanbanCard[]
}

export const KANBAN_STORAGE_KEY = 'pyths-clones-kanban'

export const KANBAN_FIXTURE: KanbanColumn[] = [
  {
    id: 'col1',
    title: 'To Do',
    cards: [
      { id: 'c1', text: 'Design landing hero' },
      { id: 'c2', text: 'Write onboarding copy' },
      { id: 'c3', text: 'Fix mobile nav overflow' },
    ],
  },
  {
    id: 'col2',
    title: 'In Progress',
    cards: [
      { id: 'c4', text: 'Implement auth flow' },
      { id: 'c5', text: 'Chart legend spacing' },
    ],
  },
  {
    id: 'col3',
    title: 'Done',
    cards: [{ id: 'c6', text: 'Set up CI pipeline' }],
  },
]
