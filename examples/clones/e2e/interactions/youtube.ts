import { expect } from '@playwright/test'
import type { InteractionScript } from './types'

// YouTube — live search filtering, IntersectionObserver infinite scroll,
// watch view with muted-autoplay <video>, subscribe toggle, related-rail
// navigation. The seek bar tracks the (real) video clock → its @value is
// volatile and excluded from snapshots; play/pause state is asserted via
// the button label, which IS diffed.

const FIRST_TITLE = 'Building a Compiler in Rust — Full Course'

export const youtubeScript: InteractionScript = {
  clone: 'youtube',
  volatileValueTestIds: ['yt-seek', 'yt-time'],
  ready: async (page) => {
    await expect(page.getByTestId('yt-app')).toBeVisible()
    await expect
      .poll(async () => page.getByTestId('yt-card').count())
      .toBeGreaterThanOrEqual(12)
  },
  steps: [
    {
      name: 'search "rust" filters the feed to 4 cards',
      run: (p) => p.getByTestId('yt-search').fill('rust'),
      settle: async (p) => {
        await expect(p.getByTestId('yt-card')).toHaveCount(4)
      },
    },
    {
      name: 'search with no matches shows the empty state',
      run: (p) => p.getByTestId('yt-search').fill('no such video xyz'),
      settle: async (p) => {
        await expect(p.getByTestId('yt-empty')).toBeVisible()
      },
    },
    {
      name: 'clearing the search restores the feed',
      run: (p) => p.getByTestId('yt-search').fill(''),
      settle: async (p) => {
        await expect.poll(async () => p.getByTestId('yt-card').count()).toBeGreaterThanOrEqual(12)
      },
    },
    {
      name: 'infinite scroll appends the next batch',
      run: (p) => p.evaluate(() => window.scrollTo(0, document.body.scrollHeight)),
      settle: async (p) => {
        await expect.poll(async () => p.getByTestId('yt-card').count(), { timeout: 10000 }).toBeGreaterThanOrEqual(24)
      },
    },
    {
      name: 'open the first card — watch view + muted autoplay engages',
      run: async (p) => {
        await p.evaluate(() => window.scrollTo(0, 0))
        await p.getByTestId('yt-card').first().click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('yt-watch')).toBeVisible()
        await expect(p.getByTestId('yt-watch-title')).toHaveText(FIRST_TITLE)
        await expect(p.getByTestId('yt-play')).toHaveText('Pause')
      },
    },
    {
      name: 'pause the video from the control',
      run: (p) => p.getByTestId('yt-play').click(),
      settle: async (p) => {
        await expect(p.getByTestId('yt-play')).toHaveText('Play')
      },
    },
    {
      name: 'subscribe toggles on',
      run: (p) => p.getByTestId('yt-subscribe').click(),
      settle: async (p) => {
        await expect(p.getByTestId('yt-subscribe')).toHaveText('Subscribed')
      },
    },
    {
      name: 'related-rail click navigates to another video',
      run: (p) => p.getByTestId('yt-related-item').first().click(),
      settle: async (p) => {
        await expect(p.getByTestId('yt-watch-title')).not.toHaveText(FIRST_TITLE)
      },
    },
    {
      name: 'back returns to the feed',
      run: (p) => p.getByTestId('yt-back').click(),
      settle: async (p) => {
        await expect(p.getByTestId('yt-watch')).toHaveCount(0)
        await expect(p.getByTestId('yt-feed')).toBeVisible()
      },
    },
  ],
}
