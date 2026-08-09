import { test, expect } from './lib/parity-test'

// Twitter clone — runs against BOTH apps (playwright.vite.config.ts /
// playwright.next.config.ts differ only in baseURL/webServer). Specs are
// written against routes + data-testids only, never app internals, so the
// same file covers the Vite shell and the Next shell (see CONTRIBUTING.md).
//
// Covered per route (production .ps island AND /react-reference oracle):
//   - IntersectionObserver infinite feed: scrolling the sentinel loads more
//   - optimistic post: appears instantly as "Sending…", then confirms
//   - "#fail" post: appears, then rolls back with an error toast
//   - like counter: optimistic toggle up/down
//   - liking the "#fail" tweet: optimistic bump, rollback + toast
//   - thread view: opens on click, back button returns to the feed

// Oracle first — see the ledger-subtraction note in lib/runtime-capture.ts.
const ROUTES = ['/react-reference/twitter', '/twitter']

for (const route of ROUTES) {
  test.describe(`Twitter clone at ${route}`, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route)
      // feed rendered (and, on the Next shell, island hydrated enough to paint)
      await expect(page.getByTestId('tweet-t1')).toBeVisible()
    })

    test('infinite feed loads more tweets when the sentinel scrolls into view', async ({
      page,
    }) => {
      await expect(page.getByTestId('feed-count')).toHaveText('10 of 66')
      await expect(page.getByTestId('tweet-t11')).toHaveCount(0)
      await page.getByTestId('feed-sentinel').scrollIntoViewIfNeeded()
      await expect(page.getByTestId('tweet-t11')).toBeVisible()
      await expect(page.getByTestId('feed-count')).not.toHaveText('10 of 66')
    })

    test('optimistic post appears instantly, then confirms', async ({ page }) => {
      await page.getByTestId('compose-input').fill('Hello from Playwright')
      await page.getByTestId('compose-post').click()
      // optimistic: visible immediately with the sending marker
      await expect(page.getByTestId('sending-local-1')).toBeVisible()
      await expect(page.getByTestId('tweet-local-1')).toBeVisible()
      // simulated send confirms: marker clears, tweet stays, no toast
      await expect(page.getByTestId('sending-local-1')).toHaveCount(0)
      await expect(page.getByTestId('tweet-local-1')).toBeVisible()
      await expect(page.getByTestId('toast')).toHaveCount(0)
    })

    test('a "#fail" post appears, then rolls back with an error toast', async ({ page }) => {
      await page.getByTestId('compose-input').fill('this deploy will #fail')
      await page.getByTestId('compose-post').click()
      await expect(page.getByTestId('tweet-local-1')).toBeVisible() // optimistic
      await expect(page.getByTestId('toast')).toHaveText('Post failed — rolled back')
      await expect(page.getByTestId('tweet-local-1')).toHaveCount(0) // rolled back
    })

    test('like counter is an optimistic toggle', async ({ page }) => {
      const like = page.getByTestId('like-t1')
      await expect(like).toHaveText('♡ 12')
      await like.click()
      await expect(like).toHaveText('♥ 13')
      await like.click()
      await expect(like).toHaveText('♡ 12')
    })

    test('liking the "#fail" tweet rolls the counter back with a toast', async ({ page }) => {
      const like = page.getByTestId('like-t3')
      await expect(like).toHaveText('♡ 7')
      await like.click()
      await expect(like).toHaveText('♥ 8') // optimistic bump
      await expect(like).toHaveText('♡ 7') // rollback after the simulated send
      await expect(page.getByTestId('toast')).toHaveText('Like failed — rolled back')
    })

    test('thread view opens from a tweet and the back button returns to the feed', async ({
      page,
    }) => {
      await expect(page.getByTestId('replies-t1')).toHaveText('💬 3')
      await page.getByTestId('replies-t1').click()
      await expect(page.getByTestId('thread')).toBeVisible()
      await expect(page.getByTestId('feed')).toHaveCount(0)
      await expect(page.getByTestId('tweet-t1')).toBeVisible() // parent
      await expect(page.getByTestId('tweet-r1')).toBeVisible()
      await expect(page.getByTestId('tweet-r3')).toBeVisible()
      await page.getByTestId('thread-back').click()
      await expect(page.getByTestId('feed')).toBeVisible()
      await expect(page.getByTestId('thread')).toHaveCount(0)
      await expect(page.getByTestId('tweet-t2')).toBeVisible()
    })
  })
}
