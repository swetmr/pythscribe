import { defineBehaviorSuite, expect, user, screen, textEntries } from './_harness'

// Contract: KanbanLite() — three columns Todo/Doing/Done; each column has
// input+Add; cards have move buttons. Core behavioral checks: all three
// columns render, and adding a card to a column appends it.
defineBehaviorSuite('macro_kanban_lite', 'KanbanLite', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // three columns present
  expect(screen.getAllByText(/\bTodo\b/i).length).toBeGreaterThan(0)
  expect(screen.getAllByText(/\bDoing\b/i).length).toBeGreaterThan(0)
  expect(screen.getAllByText(/\bDone\b/i).length).toBeGreaterThan(0)

  // per-column Add controls exist (at least 3 add buttons and 3 inputs)
  const adds = screen.getAllByRole('button', { name: /add|\+/i })
  const boxes = textEntries()
  expect(adds.length).toBeGreaterThanOrEqual(3)
  expect(boxes.length).toBeGreaterThanOrEqual(3)

  // adding a card to the first column appends the typed title
  await u.type(boxes[0] as HTMLInputElement, 'MangoCardUnique')
  await u.click(adds[0])
  expect(screen.getByText(/MangoCardUnique/i)).toBeTruthy()
  expect(container).toBeTruthy()
})
