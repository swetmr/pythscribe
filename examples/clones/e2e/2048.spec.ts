import { test, expect } from './lib/parity-test'

// 2048 clone e2e — runs against BOTH apps (playwright.vite.config.ts /
// playwright.next.config.ts) and, inside each app, against BOTH tracks:
// /2048 (PythScribe production) and /react-reference/2048 (React oracle).
// The game is keyboard-driven (Arrow keys on a window listener) and fully
// deterministic (seeded PRNG, no Math.random / Date.now), so the same key
// sequence produces byte-identical DOM on every track.
//
// Determinism: every test starts by clicking "New Game" (restart), which
// writes the fixed-seed fixture board through to localStorage. The two seed
// tiles always land at (1,2) and (3,3).

// Oracle listed FIRST: with serial workers the react-reference run records
// its runtime-error ledger before the ps run, enabling ps-minus-oracle
// subtraction in the parity-test teardown (see lib/runtime-capture.ts).
const ROUTES = [
  { label: 'React reference', path: '/react-reference/2048' },
  { label: 'PythScribe production', path: '/2048' },
]

for (const route of ROUTES) {
  test.describe(`2048 (${route.label})`, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route.path)
      await page.getByTestId('restart').click()
      await expect(page.getByTestId('moves-value')).toHaveText('0')
      await expect(page.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
    })

    test('renders the deterministic seed board with zeroed score and moves', async ({ page }) => {
      await expect(page.getByTestId('score-value')).toHaveText('0')
      await expect(page.getByTestId('moves-value')).toHaveText('0')
      await expect(page.getByTestId('cell-0-0')).toHaveAttribute('data-value', '0')
      await expect(page.getByTestId('status')).toHaveText('Use the arrow keys to combine tiles.')
      await expect(page.getByTestId('overlay')).toHaveCount(0)
    })

    test('ArrowLeft slides both seed tiles to column 0 and spawns a new tile', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await expect(page.getByTestId('moves-value')).toHaveText('1')
      await expect(page.getByTestId('cell-1-0')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('cell-3-0')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('cell-1-2')).toHaveAttribute('data-value', '0')
      await expect(page.getByTestId('score-value')).toHaveText('0')
    })

    test('ArrowLeft then ArrowUp merges two 2s into a 4 and scores 4', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowUp')
      await expect(page.getByTestId('cell-0-0')).toHaveAttribute('data-value', '4')
      await expect(page.getByTestId('cell-0-0')).toHaveText('4')
      await expect(page.getByTestId('cell-0-3')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('score-value')).toHaveText('4')
      await expect(page.getByTestId('moves-value')).toHaveText('2')
    })

    test('persists the game to localStorage and restores it across a reload', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowUp')
      await expect(page.getByTestId('score-value')).toHaveText('4')
      await page.reload()
      // saved game applied by a post-hydration effect — the retrying
      // assertion waits for it (not the fixture's 0)
      await expect(page.getByTestId('score-value')).toHaveText('4')
      await expect(page.getByTestId('moves-value')).toHaveText('2')
      await expect(page.getByTestId('cell-0-0')).toHaveAttribute('data-value', '4')
    })

    test('New Game resets the board, score and moves after some play', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowUp')
      await expect(page.getByTestId('moves-value')).toHaveText('2')
      await page.getByTestId('restart').click()
      await expect(page.getByTestId('score-value')).toHaveText('0')
      await expect(page.getByTestId('moves-value')).toHaveText('0')
      await expect(page.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
      await expect(page.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
    })
  })
}
