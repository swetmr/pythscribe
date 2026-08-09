import { expect } from '@playwright/test'
import type { InteractionScript } from './types'

// Netflix — create_portal detail modal (mounted under document.body, which
// the snapshot serializer covers by serializing <body>), Escape/backdrop
// close + body-scroll lock, hover previews, cross-carousel My List sync,
// carousel scrollBy. CSS transitions are killed at ready (same trick as
// netflix.spec.ts) so hover/scale animations never destabilize hit targets.

export const netflixScript: InteractionScript = {
  clone: 'netflix',
  ready: async (page) => {
    await expect(page.getByTestId('nf-hero-title')).toHaveText('The Dark Knight')
    await page.addStyleTag({
      content: '*, *::before, *::after { transition: none !important; }',
    })
  },
  steps: [
    {
      name: 'open the detail modal from a card (portal under <body>)',
      run: (p) => p.getByTestId('nf-card-trending-t04').click(),
      settle: async (p) => {
        await expect(p.getByTestId('nf-modal-title')).toHaveText('Pulp Fiction')
        await expect(p.getByTestId('nf-modal')).toHaveAttribute('aria-modal', 'true')
      },
    },
    {
      name: 'Escape closes the modal and unlocks body scroll',
      run: (p) => p.keyboard.press('Escape'),
      settle: async (p) => {
        await expect(p.getByTestId('nf-modal-backdrop')).toHaveCount(0)
      },
    },
    {
      name: 'add t01 to My List from the trending hover preview',
      run: async (p) => {
        await p.getByTestId('nf-card-trending-t01').hover()
        await p.getByTestId('nf-toggle-trending-t01').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('nf-card-my-list-t01')).toBeVisible()
        await expect(p.getByTestId('nf-card-acclaimed-t01')).toHaveAttribute('data-in-list', 'true')
      },
    },
    {
      name: 'add s01 via the modal toggle (context through the portal)',
      run: async (p) => {
        await p.getByTestId('nf-card-scifi-s01').click()
        await expect(p.getByTestId('nf-modal-toggle')).toHaveText('+ Add to My List')
        await p.getByTestId('nf-modal-toggle').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('nf-modal-toggle')).toHaveText('✓ In My List')
      },
    },
    {
      name: 'close the modal via backdrop click',
      run: (p) => p.getByTestId('nf-modal-backdrop').click({ position: { x: 5, y: 5 } }),
      settle: async (p) => {
        await expect(p.getByTestId('nf-modal-backdrop')).toHaveCount(0)
        await expect(p.getByTestId('nf-track-my-list').locator('.nf-card')).toHaveCount(2)
      },
    },
    {
      name: 'remove t01 from the acclaimed row — every consumer updates',
      run: async (p) => {
        await p.getByTestId('nf-card-acclaimed-t01').hover()
        await p.getByTestId('nf-toggle-acclaimed-t01').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('nf-card-my-list-t01')).toHaveCount(0)
        await expect(p.getByTestId('nf-card-trending-t01')).toHaveAttribute('data-in-list', 'false')
      },
    },
    {
      name: 'carousel scrolls right via the arrow button',
      run: (p) => p.getByTestId('nf-scroll-right-trending').click(),
      settle: async (p) => {
        await expect
          .poll(() => p.getByTestId('nf-track-trending').evaluate((el) => el.scrollLeft), { timeout: 5000 })
          .toBeGreaterThan(0)
      },
    },
    {
      name: 'carousel scrolls back left',
      run: (p) => p.getByTestId('nf-scroll-left-trending').click(),
      settle: async (p) => {
        await expect
          .poll(() => p.getByTestId('nf-track-trending').evaluate((el) => el.scrollLeft), { timeout: 5000 })
          .toBe(0)
      },
    },
  ],
}
