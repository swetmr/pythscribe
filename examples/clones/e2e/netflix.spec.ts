import { test, expect } from './lib/parity-test'

// Netflix clone e2e — framework-agnostic, runs against BOTH apps via the two
// Playwright configs (vite/next), and against BOTH tracks in each app: the
// PythScribe production route (/netflix) and the React oracle mirror
// (/react-reference/netflix). Covers the clone's stress-test features:
// carousel scrollBy, the create_portal detail modal (Escape-to-close +
// body-scroll lock), hover preview, and cross-carousel My List sync
// (fixture t01 appears in BOTH the trending and acclaimed rows).

// Oracle first — see the ledger-subtraction note in lib/runtime-capture.ts.
for (const route of ['/react-reference/netflix', '/netflix']) {
  test.describe(`Netflix clone at ${route}`, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route)
      // Kill CSS transitions (card hover-scale, preview fade) so Playwright's
      // actionability checks never sample a mid-animation bounding box —
      // the hover-reveal *behavior* is still fully asserted below.
      await page.addStyleTag({
        content: '*, *::before, *::after { transition: none !important; }',
      })
    })

    test('hero banner renders title, description and CTAs', async ({ page }) => {
      await expect(page.getByTestId('nf-hero-title')).toHaveText('The Dark Knight')
      await expect(page.getByTestId('nf-hero-desc')).toContainText('masked vigilante')
      await expect(page.getByTestId('nf-hero-play')).toBeVisible()
      await expect(page.getByTestId('nf-hero-info')).toBeVisible()
      // All four fixture rows + the (empty) My List section are on the page.
      for (const row of ['trending', 'acclaimed', 'scifi', 'comedies']) {
        await expect(page.getByTestId(`nf-row-${row}`)).toBeVisible()
      }
      await expect(page.getByTestId('nf-mylist-empty')).toBeVisible()
    })

    test('carousel scrolls right and back left via the arrow buttons', async ({ page }) => {
      const track = page.getByTestId('nf-track-trending')
      expect(await track.evaluate((el) => el.scrollLeft)).toBe(0)
      await page.getByTestId('nf-scroll-right-trending').click()
      await expect
        .poll(() => track.evaluate((el) => el.scrollLeft), { timeout: 5000 })
        .toBeGreaterThan(0)
      await page.getByTestId('nf-scroll-left-trending').click()
      await expect.poll(() => track.evaluate((el) => el.scrollLeft), { timeout: 5000 }).toBe(0)
    })

    test('hovering a card reveals the inline preview info', async ({ page }) => {
      const preview = page.getByTestId('nf-preview-trending-t02')
      await expect(preview).toBeHidden()
      await page.getByTestId('nf-card-trending-t02').hover()
      await expect(preview).toBeVisible()
      await expect(preview).toContainText('93% Match')
      await expect(preview).toContainText('2010 | 2h 04m')
    })

    test('detail modal opens via portal under document.body, locks body scroll, closes on Escape', async ({
      page,
    }) => {
      await expect(page.getByTestId('nf-modal-backdrop')).toHaveCount(0)
      await page.getByTestId('nf-card-trending-t04').click()

      const backdrop = page.getByTestId('nf-modal-backdrop')
      await expect(backdrop).toBeVisible()
      // The create_portal proof: the backdrop is a DIRECT child of <body>,
      // not of the app mount point.
      expect(await backdrop.evaluate((el) => el.parentElement === document.body)).toBe(true)
      await expect(page.getByTestId('nf-modal-title')).toHaveText('Pulp Fiction')
      await expect(page.getByTestId('nf-modal')).toHaveAttribute('aria-modal', 'true')
      // use_effect body-scroll lock while open...
      expect(await page.evaluate(() => document.body.style.overflow)).toBe('hidden')
      // ...and Escape (document-level keydown listener) closes + restores.
      await page.keyboard.press('Escape')
      await expect(page.getByTestId('nf-modal-backdrop')).toHaveCount(0)
      expect(await page.evaluate(() => document.body.style.overflow)).toBe('')
    })

    test('modal also closes on backdrop click but not on panel click', async ({ page }) => {
      await page.getByTestId('nf-hero-info').click()
      await expect(page.getByTestId('nf-modal-title')).toHaveText('The Dark Knight')
      await page.getByTestId('nf-modal-title').click()
      await expect(page.getByTestId('nf-modal')).toBeVisible()
      await page.getByTestId('nf-modal-backdrop').click({ position: { x: 5, y: 5 } })
      await expect(page.getByTestId('nf-modal-backdrop')).toHaveCount(0)
    })

    test('My List toggle syncs across carousels and the live My List row', async ({ page }) => {
      // Add t01 from the trending row (toggle lives in the hover preview).
      await page.getByTestId('nf-card-trending-t01').hover()
      await page.getByTestId('nf-toggle-trending-t01').click()

      // Reflected on the SAME title's card in a DIFFERENT carousel...
      await expect(page.getByTestId('nf-card-trending-t01')).toHaveAttribute(
        'data-in-list',
        'true',
      )
      await expect(page.getByTestId('nf-card-acclaimed-t01')).toHaveAttribute(
        'data-in-list',
        'true',
      )
      // ...and the My List row materializes live with that title.
      await expect(page.getByTestId('nf-mylist-empty')).toHaveCount(0)
      await expect(page.getByTestId('nf-card-my-list-t01')).toBeVisible()

      // Add a second title from the sci-fi row via its modal (context read
      // through the portal).
      await page.getByTestId('nf-card-scifi-s01').click()
      await expect(page.getByTestId('nf-modal-toggle')).toHaveText('+ Add to My List')
      await page.getByTestId('nf-modal-toggle').click()
      await expect(page.getByTestId('nf-modal-toggle')).toHaveText('✓ In My List')
      await page.keyboard.press('Escape')
      await expect(page.getByTestId('nf-track-my-list').locator('.nf-card')).toHaveCount(2)

      // Remove t01 from the OTHER row it appears in — every consumer updates.
      await page.getByTestId('nf-card-acclaimed-t01').hover()
      await page.getByTestId('nf-toggle-acclaimed-t01').click()
      await expect(page.getByTestId('nf-card-trending-t01')).toHaveAttribute(
        'data-in-list',
        'false',
      )
      await expect(page.getByTestId('nf-card-my-list-t01')).toHaveCount(0)
      await expect(page.getByTestId('nf-track-my-list').locator('.nf-card')).toHaveCount(1)
    })
  })
}
