import { defineBehaviorSuite, expect, user, screen, textEntries } from './_harness'

// Contract: VideoGrid() — search input filters >=8 videos by case-insensitive
// title; 'All' chip clears category. Data-agnostic checks: a no-match query
// shrinks the rendered set (and/or shows an empty state), clearing restores it,
// and an 'All' category chip exists.
defineBehaviorSuite('macro_video_grid', 'VideoGrid', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  const nodes = () => container.getElementsByTagName('*').length
  // tolerance: search inputs are legitimately textbox OR searchbox
  const box = textEntries()[0] as HTMLInputElement
  expect(box).toBeTruthy()
  const initial = nodes()

  // an 'All' chip exists (category clear control; label may carry a count)
  expect(screen.getAllByText(/^\s*All\b/i).length).toBeGreaterThan(0)

  // a query that matches no title collapses the grid (fewer nodes or empty state)
  await u.type(box, 'zzqqxjkvnope')
  const filtered = nodes()
  expect(filtered).toBeLessThan(initial)

  // clearing the query restores the full grid
  await u.clear(box)
  expect(nodes()).toBeGreaterThan(filtered)
})
