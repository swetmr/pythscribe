import { test, expect } from './lib/parity-test'
import type { Page } from '@playwright/test'
import { PLAYLISTS, SHUFFLE_SEED, seededShuffle } from '../shared/spotify/fixtures'

// Runs against BOTH apps (playwright.vite.config.ts / playwright.next.config.ts —
// same spec, different baseURL/webServer), and within each app against BOTH the
// production PythScribe route (/spotify) and the React oracle mirror
// (/react-reference/spotify). Playback assertions go through page.evaluate on
// the real <audio> element (offline asset /media/sample.wav, ~1s tone).

const tracks = PLAYLISTS[0].tracks

function audioState(page: Page) {
  return page.evaluate(() => {
    const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
    return { paused: a.paused, currentTime: a.currentTime, volume: a.volume }
  })
}

function waitForPlaying(page: Page) {
  return page.waitForFunction(() => {
    const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
    return a && !a.paused && a.currentTime > 0
  })
}

// Freeze auto-advance: with loop=true the `ended` event never fires, so the
// queue index only moves via explicit user actions — deterministic tests.
// (loop is not a React-managed prop here, so it survives re-renders.)
function suppressEnded(page: Page) {
  return page.evaluate(() => {
    const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
    a.loop = true
  })
}

// Oracle first — see the ledger-subtraction note in lib/runtime-capture.ts.
for (const { label, route } of [
  { label: 'React reference', route: '/react-reference/spotify' },
  { label: 'production (PythScribe)', route: '/spotify' },
]) {
  test.describe(`Spotify clone — ${label}`, () => {
    test('renders sidebar playlists, track table and idle player bar', async ({ page }) => {
      await page.goto(route)
      for (const pl of PLAYLISTS) {
        await expect(page.getByTestId(`playlist-${pl.id}`)).toBeVisible()
      }
      await expect(page.locator('[data-testid^="track-row-"]')).toHaveCount(tracks.length)
      await expect(page.getByTestId('player-track-title')).toHaveText('Nothing playing')
      await expect(page.getByTestId('queue-empty')).toBeVisible()
      // duration column formatted mm:ss
      await expect(page.getByTestId(`track-row-${tracks[0].id}`)).toContainText('3:34')
    })

    test('row play drives the real audio element; player pause freezes it', async ({ page }) => {
      await page.goto(route)
      await suppressEnded(page) // ~1s asset: keep the queue index stable during assertions
      await page.getByTestId(`track-row-${tracks[0].id}`).hover()
      await page.getByTestId(`track-play-${tracks[0].id}`).click()
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[0].title)
      await expect(page.getByTestId(`track-row-${tracks[0].id}`)).toHaveAttribute('data-active', 'true')
      // REAL playback: not paused and the clock advances
      await waitForPlaying(page)
      // seek bar is bound to timeupdate — its value follows the audio clock
      await page.waitForFunction(() => {
        const s = document.querySelector('[data-testid="player-seek"]') as HTMLInputElement
        return parseFloat(s.value) > 0
      })
      // pause from the player bar
      await page.getByTestId('player-play').click()
      const s1 = await audioState(page)
      expect(s1.paused).toBe(true)
      await page.waitForTimeout(300)
      const s2 = await audioState(page)
      expect(s2.paused).toBe(true)
      expect(s2.currentTime).toBe(s1.currentTime) // frozen while paused
      // resume
      await page.getByTestId('player-play').click()
      await waitForPlaying(page)
    })

    test('next/prev honor the queue while audio keeps playing', async ({ page }) => {
      await page.goto(route)
      await suppressEnded(page)
      await page.getByTestId(`track-row-${tracks[0].id}`).hover()
      await page.getByTestId(`track-play-${tracks[0].id}`).click()
      await waitForPlaying(page)
      await page.getByTestId('player-next').click()
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[1].title)
      await page.getByTestId('player-next').click()
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[2].title)
      await page.getByTestId('player-prev').click()
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[1].title)
      await waitForPlaying(page) // still playing after queue moves
      // up-next reflects the queue position
      const upNext = page.locator('[data-testid="queue-item"]')
      await expect(upNext).toHaveText(tracks.slice(2).map((t) => t.title))
    })

    test('add-to-queue from the row menu, then seeded shuffle gives the exact expected order', async ({ page }) => {
      await page.goto(route)
      // build a 4-track queue from idle (no playback -> fully deterministic)
      const picks = [tracks[0], tracks[4], tracks[2], tracks[6]]
      for (const t of picks) {
        await page.getByTestId(`track-row-${t.id}`).hover()
        await page.getByTestId(`track-menu-${t.id}`).click()
        await page.getByTestId(`queue-add-${t.id}`).click()
      }
      const items = page.locator('[data-testid="queue-item"]')
      await expect(items).toHaveText(picks.map((t) => t.title))
      // deterministic seeded shuffle (fixed seed, idle -> whole queue reshuffles)
      await page.getByTestId('player-shuffle').click()
      await expect(page.getByTestId('player-shuffle')).toHaveAttribute('aria-pressed', 'true')
      const expected = seededShuffle(picks, SHUFFLE_SEED).map((t) => t.title)
      await expect(items).toHaveText(expected)
      // pressing play starts the shuffled queue and really plays audio
      await page.getByTestId('player-play').click()
      await expect(page.getByTestId('player-track-title')).toHaveText(expected[0])
      await waitForPlaying(page)
    })

    test('auto-advances to the next track when the audio really ends', async ({ page }) => {
      await page.goto(route)
      await page.getByTestId(`track-row-${tracks[0].id}`).hover()
      await page.getByTestId(`track-play-${tracks[0].id}`).click()
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[0].title)
      // sample.wav is ~1s — the real `ended` event must advance the queue
      await expect(page.getByTestId('player-track-title')).toHaveText(tracks[1].title, { timeout: 15000 })
      const s = await audioState(page)
      expect(s.paused).toBe(false) // next track keeps playing
    })

    test('Space toggles playback, but not while focus is in an input', async ({ page }) => {
      await page.goto(route)
      await suppressEnded(page)
      await page.getByTestId(`track-row-${tracks[0].id}`).hover()
      await page.getByTestId(`track-play-${tracks[0].id}`).click()
      await waitForPlaying(page)
      // move focus off the row button so Space hits the window handler only
      await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())
      await page.keyboard.press('Space')
      await expect(page.getByTestId('player-play')).toHaveAccessibleName('Play')
      expect((await audioState(page)).paused).toBe(true)
      await page.keyboard.press('Space')
      await expect(page.getByTestId('player-play')).toHaveAccessibleName('Pause')
      await waitForPlaying(page)
      // focus the volume slider (an <input>): Space must NOT toggle
      await page.getByTestId('player-volume').focus()
      await page.keyboard.press('Space')
      await page.waitForTimeout(200)
      await expect(page.getByTestId('player-play')).toHaveAccessibleName('Pause')
      expect((await audioState(page)).paused).toBe(false)
    })

    test('volume slider sets the audio element volume', async ({ page }) => {
      await page.goto(route)
      // The Next shell server-renders the <audio> before React hydrates, so
      // volume is the browser default (1) until the volume effect applies the
      // reducer's initial 0.8 — wait for hydration instead of asserting instantly.
      await page.waitForFunction(() => {
        const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
        return a && Math.abs(a.volume - 0.8) < 1e-6
      })
      // real keyboard interaction: ArrowLeft steps the range input by one
      // `step` (0.01) and fires the change the reducer listens to
      await page.getByTestId('player-volume').focus()
      await page.keyboard.press('ArrowLeft')
      expect((await audioState(page)).volume).toBeCloseTo(0.79, 5)
    })
  })
}
