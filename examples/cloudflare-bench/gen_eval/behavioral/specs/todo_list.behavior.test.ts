import { defineBehaviorSuite, expect, user, screen, textEntries } from './_harness'

// Contract: TodoApp() — text input + Add appends (ignore empty, clear input
// after add); clicking a todo toggles done; footer counts not-done.
defineBehaviorSuite('macro_todo_list', 'TodoApp', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  const box = textEntries()[0] as HTMLInputElement
  const addBtn = screen.getByRole('button', { name: /add/i })

  // empty Add is ignored
  const before = screen.queryAllByText(/ZebraTaskUnique/i).length
  await u.click(addBtn)
  expect(screen.queryAllByText(/ZebraTaskUnique/i).length).toBe(before)

  // typing + Add appends the item and clears the input
  await u.type(box, 'ZebraTaskUnique')
  await u.click(addBtn)
  expect(screen.getByText(/ZebraTaskUnique/i)).toBeTruthy()
  expect((textEntries()[0] as HTMLInputElement).value).toBe('')

  // clicking the todo toggles its done state (observable DOM change)
  const htmlBefore = container.innerHTML
  await u.click(screen.getByText(/ZebraTaskUnique/i))
  expect(container.innerHTML).not.toBe(htmlBefore)
})
