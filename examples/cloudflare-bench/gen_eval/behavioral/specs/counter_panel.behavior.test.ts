import { defineBehaviorSuite, expect, user, screen } from './_harness'

// Contract: CounterPanel() — count starts 0; increment/decrement/reset;
// message shows 'even'/'odd'; decrement disabled at 0.
defineBehaviorSuite('macro_counter_panel', 'CounterPanel', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // starts at 0 and reads 'even'
  expect(screen.getAllByText(/\b0\b/).length).toBeGreaterThan(0)
  expect(screen.getByText(/\beven\b/i)).toBeTruthy()

  // decrement disabled at 0
  const dec = screen.getByRole('button', { name: /decrement|−|-|minus/i })
  expect((dec as HTMLButtonElement).disabled).toBe(true)

  // increment -> value 1, parity odd, decrement now enabled
  await u.click(screen.getByRole('button', { name: /increment|\+|plus/i }))
  expect(screen.getAllByText(/\b1\b/).length).toBeGreaterThan(0)
  expect(screen.getByText(/\bodd\b/i)).toBeTruthy()
  expect((screen.getByRole('button', { name: /decrement|−|-|minus/i }) as HTMLButtonElement).disabled).toBe(false)

  // reset -> back to 0
  await u.click(screen.getByRole('button', { name: /reset/i }))
  expect(screen.getAllByText(/\b0\b/).length).toBeGreaterThan(0)
  expect(container).toBeTruthy()
})
