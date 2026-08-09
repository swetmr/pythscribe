import { defineBehaviorSuite, expect, user, screen } from './_harness'

// Contract: MovieBrowser() — hero (featured title+desc); Trending/New rows of
// cards; clicking a card opens an inline detail panel with a Close button.
defineBehaviorSuite('macro_movie_rows', 'MovieBrowser', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // row labels present
  expect(screen.getAllByText(/trending/i).length).toBeGreaterThan(0)
  expect(screen.getAllByText(/\bnew\b/i).length).toBeGreaterThan(0)

  const closeRe = /close|✕|×|✖/i
  // no detail panel open initially
  expect(screen.queryByRole('button', { name: closeRe })).toBeNull()

  // Clicking a movie card opens the detail panel. Cards show a 4-digit year;
  // click each year-bearing element (bubbles to the card handler) until a
  // Close button appears — this skips the non-clickable hero year.
  const years = Array.from(container.querySelectorAll<HTMLElement>('*'))
    .filter((el) => /\b(19|20)\d{2}\b/.test(el.textContent || ''))
    .sort((a, b) => (a.textContent || '').length - (b.textContent || '').length)
    .slice(0, 24)
  expect(years.length).toBeGreaterThan(0)
  let opened = false
  for (const y of years) {
    await u.click(y)
    if (screen.queryByRole('button', { name: closeRe })) { opened = true; break }
  }
  expect(opened).toBe(true)

  // closing dismisses the panel (single panel at a time)
  await u.click(screen.getByRole('button', { name: closeRe }))
  expect(screen.queryByRole('button', { name: closeRe })).toBeNull()
})
