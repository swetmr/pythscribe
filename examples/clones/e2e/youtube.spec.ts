import { test, expect } from './lib/parity-test'
import type { Page } from '@playwright/test'

// YouTube clone e2e — runs against BOTH apps (playwright.vite.config.ts /
// playwright.next.config.ts supply baseURL/webServer; relative paths only)
// and against both tracks per app: /youtube (PythScribe production) and
// /react-reference/youtube (React oracle mirror). Asserts real behavior:
// search filters the feed, IntersectionObserver infinite scroll appends
// batches, the <video> actually plays (checked via the element's
// paused/currentTime), keyboard shortcuts drive the player, subscribe and
// related-rail state work.

// Oracle first — see the ledger-subtraction note in lib/runtime-capture.ts.
const ROUTES = [
  { label: 'React reference', path: '/react-reference/youtube' },
  { label: 'production (PythScribe)', path: '/youtube' },
]

const FIRST_TITLE = 'Building a Compiler in Rust — Full Course'

async function scrollToBottom(page: Page) {
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
}

for (const route of ROUTES) {
  test.describe(`YouTube clone — ${route.label}`, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route.path)
      await expect(page.getByTestId('yt-app')).toBeVisible()
    })

    test('feed renders the first batch and infinite scroll appends until exhausted', async ({ page }) => {
      const cards = page.getByTestId('yt-card')
      // First batch is 12; the sentinel's 200px rootMargin may legitimately
      // auto-load one more batch on tall viewports — but never all 48.
      await expect
        .poll(async () => cards.count())
        .toBeGreaterThanOrEqual(12)
      const initial = await cards.count()
      expect(initial).toBeLessThan(48)

      // Scroll the sentinel into view repeatedly; batches of 12 append
      // until all 48 fixture videos are shown and the sentinel unmounts.
      await expect
        .poll(
          async () => {
            await scrollToBottom(page)
            return cards.count()
          },
          { timeout: 15000 },
        )
        .toBe(48)
      await expect(page.getByTestId('yt-sentinel')).toHaveCount(0)
    })

    test('search filters the feed live and the empty state appears for no matches', async ({ page }) => {
      const search = page.getByTestId('yt-search')
      await search.fill('rust')
      const titles = page.getByTestId('yt-card-title')
      await expect(page.getByTestId('yt-card')).toHaveCount(4)
      for (const text of await titles.allTextContents()) {
        expect(text.toLowerCase()).toContain('rust')
      }

      await search.fill('no such video xyz')
      await expect(page.getByTestId('yt-card')).toHaveCount(0)
      await expect(page.getByTestId('yt-empty')).toBeVisible()

      await search.fill('')
      await expect
        .poll(async () => page.getByTestId('yt-card').count())
        .toBeGreaterThanOrEqual(12)
    })

    test('opening a card switches to the watch view and the <video> really plays', async ({ page }) => {
      // The player is `autoPlay muted` (like the real site), so the test
      // must NOT assume a paused start — that was a race: in a warm
      // browser (full-suite runs) autoplay wins before the assertion, in
      // isolation the assertion wins (#134). Asserting autoplay is the
      // STRONGER claim: the <video> must start playing on its own.
      await page.getByTestId('yt-card').first().click()
      await expect(page.getByTestId('yt-watch')).toBeVisible()
      await expect(page.getByTestId('yt-watch-title')).toHaveText(FIRST_TITLE)

      const video = page.getByTestId('yt-video')
      // Autoplay really plays: state button flips to Pause and media time advances.
      await expect(page.getByTestId('yt-play')).toHaveText('Pause')
      await expect
        .poll(() => video.evaluate((v: HTMLVideoElement) => !v.paused && v.currentTime > 0))
        .toBe(true)

      // Seek bar is bound to timeUpdate: the range input's value advances.
      await expect
        .poll(() => page.getByTestId('yt-seek').evaluate((el: HTMLInputElement) => parseFloat(el.value)))
        .toBeGreaterThan(0)

      // Pause stops it...
      await page.getByTestId('yt-play').click()
      await expect(page.getByTestId('yt-play')).toHaveText('Play')
      expect(await video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(true)

      // ...and Play resumes it.
      await page.getByTestId('yt-play').click()
      await expect(page.getByTestId('yt-play')).toHaveText('Pause')
      await expect
        .poll(() => video.evaluate((v: HTMLVideoElement) => !v.paused))
        .toBe(true)
    })

    test('keyboard shortcuts: space toggles play/pause, arrows seek', async ({ page }) => {
      await page.getByTestId('yt-card').first().click()
      const video = page.getByTestId('yt-video')

      // Deterministic baseline: wait for the muted autoplay to engage
      // (see the autoplay note in the previous test — #134).
      await expect(page.getByTestId('yt-play')).toHaveText('Pause')
      await expect
        .poll(() => video.evaluate((v: HTMLVideoElement) => !v.paused))
        .toBe(true)

      // Space pauses (video is now stationary — seek asserts are exact).
      await page.keyboard.press('Space')
      await expect(page.getByTestId('yt-play')).toHaveText('Play')
      expect(await video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(true)

      // ArrowRight seeks +5s, clamped to the (short) clip duration.
      await page.keyboard.press('ArrowRight')
      await expect
        .poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime))
        .toBeGreaterThan(0.5)
      // ArrowLeft seeks back down to 0.
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowLeft')
      expect(await video.evaluate((v: HTMLVideoElement) => v.currentTime)).toBe(0)

      // Space resumes.
      await page.keyboard.press('Space')
      await expect(page.getByTestId('yt-play')).toHaveText('Pause')
      await expect
        .poll(() => video.evaluate((v: HTMLVideoElement) => !v.paused))
        .toBe(true)
    })

    test('subscribe toggles, related rail navigates, back returns to the feed', async ({ page }) => {
      await page.getByTestId('yt-card').first().click()

      const subscribe = page.getByTestId('yt-subscribe')
      await expect(subscribe).toHaveText('Subscribe')
      await subscribe.click()
      await expect(subscribe).toHaveText('Subscribed')
      await subscribe.click()
      await expect(subscribe).toHaveText('Subscribe')

      await expect(page.getByTestId('yt-related-item')).toHaveCount(10)
      const secondTitle = await page.getByTestId('yt-related-item').first().locator('.yt-related-title').textContent()
      await page.getByTestId('yt-related-item').first().click()
      await expect(page.getByTestId('yt-watch-title')).toHaveText(secondTitle!)

      await page.getByTestId('yt-back').click()
      await expect(page.getByTestId('yt-watch')).toHaveCount(0)
      await expect(page.getByTestId('yt-feed')).toBeVisible()
    })

    test('hovering a card shows the autoplaying preview and unhover removes it', async ({ page }) => {
      const firstCard = page.getByTestId('yt-card').first()
      await expect(page.getByTestId('yt-preview')).toHaveCount(0)
      await firstCard.hover()
      await expect(firstCard.getByTestId('yt-preview')).toBeVisible()
      await page.mouse.move(0, 0)
      await expect(page.getByTestId('yt-preview')).toHaveCount(0)
    })
  })
}
