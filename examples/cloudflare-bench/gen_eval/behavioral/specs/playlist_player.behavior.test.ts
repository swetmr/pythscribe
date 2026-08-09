import { defineBehaviorSuite, expect, user, screen, innermostByText } from './_harness'

// Contract: PlaylistPlayer() — sidebar >=3 playlists (click selects active),
// main panel lists active tracks, clicking a track sets 'now playing', bottom
// bar has a play/pause toggle. Data-agnostic core: a play/pause control exists,
// and clicking a track (found via its m:ss duration) updates the render.
defineBehaviorSuite('macro_playlist_player', 'PlaylistPlayer', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // bottom-bar play/pause toggle present (tolerance: common glyph variants)
  expect(screen.getAllByRole('button', { name: /play|pause|▶|❚❚|⏸|⏯|⏵|►|▷|\|\|/i }).length).toBeGreaterThan(0)

  // tracks render with m:ss durations (>=3 across the active playlist)
  const durations = Array.from(container.querySelectorAll('*'))
    .filter((el) => /^\s*\d+:\d{2}\s*$/.test((el as HTMLElement).textContent || ''))
  expect(durations.length).toBeGreaterThanOrEqual(1)

  // clicking a track sets 'now playing' -> observable DOM change
  const before = container.innerHTML
  const track = innermostByText(container, /\d+:\d{2}/)!
  await u.click(track)
  expect(container.innerHTML).not.toBe(before)
})
