import { defineBehaviorSuite, expect, user } from './_harness'
import { screen, within } from '@testing-library/react'

// Contract: HelloCard(title, subtitle=None) — heading shows title; subtitle
// paragraph only when passed; like button toggles 'Like' <-> 'Liked' on click.
defineBehaviorSuite('macro_hello_card', 'HelloCard', async ({ mount }) => {
  const u = user()

  // (a) title renders; subtitle renders when passed
  const { unmount } = mount({ title: 'Greetings Alpha', subtitle: 'Sub Beta' })
  expect(screen.getByText(/Greetings Alpha/i)).toBeTruthy()
  expect(screen.getByText(/Sub Beta/i)).toBeTruthy()

  // (b) like button toggles label Like -> Liked -> Like
  const btn = screen.getByRole('button', { name: /like/i })
  expect(btn.textContent || '').toMatch(/like/i)
  await u.click(btn)
  expect(screen.getByRole('button', { name: /liked/i }).textContent || '').toMatch(/liked/i)
  await u.click(screen.getByRole('button', { name: /liked/i }))
  expect(screen.getByRole('button', { name: /^.*\blike\b.*$/i }).textContent || '').toMatch(/like/i)
  unmount()

  // (c) subtitle omitted when not passed
  mount({ title: 'Only Title Here' })
  expect(screen.getByText(/Only Title Here/i)).toBeTruthy()
  expect(screen.queryByText(/Sub Beta/i)).toBeNull()
})
