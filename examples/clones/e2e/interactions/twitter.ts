import { expect } from '@playwright/test'
import type { InteractionScript } from './types'

// Twitter — the single best runtime-state target: compiled `async def`
// send, Promise(lambda ...) + setTimeout interop, optimistic post/like with
// rollback, toast auto-dismiss timers.
//
// Playwright's fake clock (page.clock) is installed on BOTH pages so the
// SEND_MS (600ms) sends and TOAST_MS (3000ms) auto-dismiss fire in lockstep:
// mid-flight optimistic states become STABLE snapshot points instead of
// races, and each timer-driven transition is its own diffable step.

export const twitterScript: InteractionScript = {
  clone: 'twitter',
  useClock: true,
  ready: async (page) => {
    await expect(page.getByTestId('tweet-t1')).toBeVisible()
  },
  steps: [
    {
      name: 'type a post into the composer',
      run: (p) => p.getByTestId('compose-input').fill('Lockstep parity post'),
      settle: async (p) => {
        await expect(p.getByTestId('compose-input')).toHaveValue('Lockstep parity post')
      },
    },
    {
      name: 'post — optimistic tweet appears as Sending…',
      run: (p) => p.getByTestId('compose-post').click(),
      settle: async (p) => {
        await expect(p.getByTestId('sending-local-1')).toBeVisible()
        await expect(p.getByTestId('tweet-local-1')).toBeVisible()
      },
    },
    {
      name: 'advance SEND_MS — send confirms, marker clears',
      run: async () => {},
      advanceClock: 650,
      settle: async (p) => {
        await expect(p.getByTestId('sending-local-1')).toHaveCount(0)
        await expect(p.getByTestId('tweet-local-1')).toBeVisible()
        await expect(p.getByTestId('toast')).toHaveCount(0)
      },
    },
    {
      name: 'compose a "#fail" post — optimistic appearance',
      run: async (p) => {
        await p.getByTestId('compose-input').fill('this deploy will #fail')
        await p.getByTestId('compose-post').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('tweet-local-2')).toBeVisible()
      },
    },
    {
      name: 'advance SEND_MS — rollback + error toast',
      run: async () => {},
      advanceClock: 650,
      settle: async (p) => {
        await expect(p.getByTestId('toast')).toHaveText('Post failed — rolled back')
        await expect(p.getByTestId('tweet-local-2')).toHaveCount(0)
      },
    },
    {
      name: 'advance TOAST_MS — toast auto-dismisses',
      run: async () => {},
      advanceClock: 3100,
      settle: async (p) => {
        await expect(p.getByTestId('toast')).toHaveCount(0)
      },
    },
    {
      name: 'like t1 — optimistic bump',
      run: (p) => p.getByTestId('like-t1').click(),
      settle: async (p) => {
        await expect(p.getByTestId('like-t1')).toHaveText('♥ 13')
      },
    },
    {
      name: 'advance SEND_MS — like confirms (stays 13)',
      run: async () => {},
      advanceClock: 650,
      settle: async (p) => {
        await expect(p.getByTestId('like-t1')).toHaveText('♥ 13')
        await expect(p.getByTestId('toast')).toHaveCount(0)
      },
    },
    {
      name: 'like the "#fail" tweet t3 — optimistic bump',
      run: (p) => p.getByTestId('like-t3').click(),
      settle: async (p) => {
        await expect(p.getByTestId('like-t3')).toHaveText('♥ 8')
      },
    },
    {
      name: 'advance SEND_MS — like rolls back + toast',
      run: async () => {},
      advanceClock: 650,
      settle: async (p) => {
        await expect(p.getByTestId('like-t3')).toHaveText('♡ 7')
        await expect(p.getByTestId('toast')).toHaveText('Like failed — rolled back')
      },
    },
    {
      name: 'advance TOAST_MS — second toast dismisses',
      run: async () => {},
      advanceClock: 3100,
      settle: async (p) => {
        await expect(p.getByTestId('toast')).toHaveCount(0)
      },
    },
    {
      name: 'infinite feed — sentinel loads the next batch',
      run: (p) => p.getByTestId('feed-sentinel').scrollIntoViewIfNeeded(),
      settle: async (p) => {
        await expect(p.getByTestId('tweet-t11')).toBeVisible()
      },
    },
    {
      name: 'open thread view from t1 replies',
      run: (p) => p.getByTestId('replies-t1').click(),
      settle: async (p) => {
        await expect(p.getByTestId('thread')).toBeVisible()
        await expect(p.getByTestId('tweet-r1')).toBeVisible()
      },
    },
    {
      name: 'back button returns to the feed',
      run: (p) => p.getByTestId('thread-back').click(),
      settle: async (p) => {
        await expect(p.getByTestId('feed')).toBeVisible()
        await expect(p.getByTestId('thread')).toHaveCount(0)
      },
    },
  ],
}
