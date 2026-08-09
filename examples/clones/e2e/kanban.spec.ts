import { test, expect } from './lib/parity-test'
import type { Page } from '@playwright/test'

// Kanban clone e2e — runs against BOTH apps (playwright.vite.config.ts /
// playwright.next.config.ts) and, inside each app, against BOTH tracks:
// /kanban (PythScribe production) and /react-reference/kanban (React
// oracle). The drag suite drives RAW POINTER EVENTS via page.mouse — the
// board uses no dnd library (deliberate PythScribe stress test).
//
// Determinism: every test starts by clicking the reset button, which
// clears localStorage state and restores the fixtures (col1: c1,c2,c3 /
// col2: c4,c5 / col3: c6).

// Oracle listed FIRST: with serial workers the react-reference run records
// its runtime-error ledger before the ps run, enabling ps-minus-oracle
// subtraction in the parity-test teardown (see lib/runtime-capture.ts).
const ROUTES = [
  { label: 'React reference', path: '/react-reference/kanban' },
  { label: 'PythScribe production', path: '/kanban' },
]

async function cardIds(page: Page, colId: string): Promise<(string | null)[]> {
  return page
    .getByTestId('kb-col-' + colId)
    .locator('[data-card-id]')
    .evaluateAll((els) => els.map((el) => el.getAttribute('data-card-id')))
}

async function center(page: Page, testId: string): Promise<{ x: number; y: number; w: number; h: number }> {
  const box = await page.getByTestId(testId).boundingBox()
  if (!box) throw new Error('no bounding box for ' + testId)
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, w: box.width, h: box.height }
}

for (const route of ROUTES) {
  test.describe(`Kanban (${route.label})`, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route.path)
      await page.getByTestId('kb-reset').click()
      await expect(page.getByTestId('kb-count-col1')).toHaveText('3')
    })

    test('renders the fixture board with columns, cards and count badges', async ({ page }) => {
      await expect(page.getByTestId('kb-col-title-col1')).toHaveText('To Do')
      await expect(page.getByTestId('kb-col-title-col2')).toHaveText('In Progress')
      await expect(page.getByTestId('kb-col-title-col3')).toHaveText('Done')
      await expect(page.getByTestId('kb-count-col2')).toHaveText('2')
      await expect(page.getByTestId('kb-count-col3')).toHaveText('1')
      expect(await cardIds(page, 'col1')).toEqual(['c1', 'c2', 'c3'])
    })

    test('pointer drag moves a card across columns (ghost + dropzone highlight) and persists across reload', async ({
      page,
    }) => {
      const from = await center(page, 'kb-card-c1')
      await page.mouse.move(from.x, from.y)
      await page.mouse.down()
      // past the 5px activation threshold — ghost appears
      await page.mouse.move(from.x + 12, from.y + 12, { steps: 3 })
      await expect(page.getByTestId('kb-ghost')).toBeVisible()
      await expect(page.getByTestId('kb-ghost')).toHaveText('Design landing hero')

      // hover the TOP half of c4 in col2 → insertion index 0
      const target = await center(page, 'kb-card-c4')
      await page.mouse.move(target.x, target.y - target.h / 2 + 4, { steps: 10 })
      await expect(page.getByTestId('kb-col-col2')).toHaveClass(/kb-col--drop/)
      await expect(page.getByTestId('kb-col-col1')).not.toHaveClass(/kb-col--drop/)

      await page.mouse.up()
      await expect(page.getByTestId('kb-ghost')).toHaveCount(0)
      expect(await cardIds(page, 'col1')).toEqual(['c2', 'c3'])
      expect(await cardIds(page, 'col2')).toEqual(['c1', 'c4', 'c5'])
      await expect(page.getByTestId('kb-count-col1')).toHaveText('2')
      await expect(page.getByTestId('kb-count-col2')).toHaveText('3')

      // localStorage persistence — await the retrying badge assertion first
      // (the saved board is applied by a post-hydration effect; the raw
      // evaluateAll below does not auto-retry)
      await page.reload()
      await expect(page.getByTestId('kb-count-col2')).toHaveText('3')
      expect(await cardIds(page, 'col2')).toEqual(['c1', 'c4', 'c5'])
    })

    test('pointer drag reorders a card within its column', async ({ page }) => {
      const from = await center(page, 'kb-card-c1')
      await page.mouse.move(from.x, from.y)
      await page.mouse.down()
      await page.mouse.move(from.x + 12, from.y + 12, { steps: 3 })

      // hover the BOTTOM half of c3 → insert after it (index 2 among c2,c3)
      const target = await center(page, 'kb-card-c3')
      await page.mouse.move(target.x, target.y + target.h / 2 - 4, { steps: 10 })
      await expect(page.getByTestId('kb-col-col1')).toHaveClass(/kb-col--drop/)
      await page.mouse.up()

      expect(await cardIds(page, 'col1')).toEqual(['c2', 'c3', 'c1'])
      await expect(page.getByTestId('kb-count-col1')).toHaveText('3')
    })

    test('Escape cancels an in-flight drag and restores the original position', async ({ page }) => {
      const from = await center(page, 'kb-card-c4')
      await page.mouse.move(from.x, from.y)
      await page.mouse.down()
      await page.mouse.move(from.x + 12, from.y + 12, { steps: 3 })
      const target = await center(page, 'kb-card-c6')
      await page.mouse.move(target.x, target.y, { steps: 10 })
      await expect(page.getByTestId('kb-ghost')).toBeVisible()

      await page.keyboard.press('Escape')
      await expect(page.getByTestId('kb-ghost')).toHaveCount(0)
      await page.mouse.up()

      expect(await cardIds(page, 'col2')).toEqual(['c4', 'c5'])
      expect(await cardIds(page, 'col3')).toEqual(['c6'])
      await expect(page.getByTestId('kb-count-col3')).toHaveText('1')
    })

    test('click opens the inline editor; Enter saves; Escape cancels', async ({ page }) => {
      await page.getByTestId('kb-card-c6').click()
      const edit = page.getByTestId('kb-edit-input')
      await expect(edit).toHaveValue('Set up CI pipeline')
      await edit.fill('Set up CD pipeline')
      await edit.press('Enter')
      await expect(page.getByTestId('kb-card-c6')).toContainText('Set up CD pipeline')

      // Escape cancels without saving
      await page.getByTestId('kb-card-c6').click()
      await page.getByTestId('kb-edit-input').fill('discarded text')
      await page.getByTestId('kb-edit-input').press('Escape')
      await expect(page.getByTestId('kb-card-c6')).toContainText('Set up CD pipeline')
    })

    test('composer adds a card and the count badge updates', async ({ page }) => {
      await page.getByTestId('kb-add-card-col3').click()
      await page.getByTestId('kb-compose-input').fill('Ship the kanban clone')
      await page.getByTestId('kb-compose-add').click()
      await expect(page.getByTestId('kb-card-c7')).toContainText('Ship the kanban clone')
      await expect(page.getByTestId('kb-count-col3')).toHaveText('2')
    })
  })
}
