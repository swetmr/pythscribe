import { test, expect, type Page } from './lib/parity-test'

// Tetris clone e2e — runs against BOTH tracks inside each app: /tetris
// (PythScribe production) and /react-reference/tetris (React oracle). The game
// is keyboard-driven with a seeded 7-bag (no Math.random / Date.now) and a
// setTimeout-driven gravity/lock loop, so the same key sequence + same clock
// advances produce byte-identical DOM on every track.
//
// Gravity fires every GRAVITY_MS on a real timer, so every test installs
// Playwright's fake clock (page.clock) BEFORE navigation and advances it
// explicitly — otherwise wall-clock gravity would race the assertions. Same
// seed ⇒ first piece is a T, next queue "LJSOZ".

const ROUTES = [
  { label: 'React reference', path: '/react-reference/tetris' },
  { label: 'PythScribe production', path: '/tetris' },
]

function color(page: Page, r: number, c: number) {
  return page.getByTestId('cell-' + r + '-' + c)
}

for (const route of ROUTES) {
  test.describe(`tetris (${route.label})`, () => {
    test.beforeEach(async ({ page }) => {
      await page.clock.install()
      await page.goto(route.path)
      await page.getByTestId('restart').click()
      await expect(page.getByTestId('next-queue')).toHaveText('LJSOZ')
    })

    test('renders the deterministic seed board (T piece, zeroed stats)', async ({ page }) => {
      await expect(page.getByTestId('score-value')).toHaveText('0')
      await expect(page.getByTestId('lines-value')).toHaveText('0')
      await expect(page.getByTestId('level-value')).toHaveText('1')
      await expect(page.getByTestId('hold-label')).toHaveText('-')
      await expect(color(page, 0, 4)).toHaveAttribute('data-color', '3')
      await expect(color(page, 1, 3)).toHaveAttribute('data-color', '3')
      await expect(color(page, 0, 0)).toHaveAttribute('data-filled', 'false')
      await expect(page.getByTestId('overlay')).toHaveCount(0)
    })

    test('gravity drops the piece one row per GRAVITY_MS tick', async ({ page }) => {
      await page.clock.runFor(1000)
      await expect(color(page, 2, 3)).toHaveAttribute('data-color', '3')
      await expect(color(page, 0, 4)).toHaveAttribute('data-filled', 'false')
      await page.clock.runFor(1000)
      await expect(color(page, 3, 3)).toHaveAttribute('data-color', '3')
    })

    test('rotate + move + hard drop locks the T and spawns the L', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowUp')
      await page.keyboard.press('Space')
      await expect(page.getByTestId('score-value')).toHaveText('34')
      await expect(page.getByTestId('next-queue')).toHaveText('JSOZI')
      await expect(color(page, 19, 3)).toHaveAttribute('data-color', '3')
      await expect(color(page, 0, 5)).toHaveAttribute('data-color', '7')
    })

    test('hold swaps the active piece into the hold slot', async ({ page }) => {
      await page.keyboard.press('c')
      await expect(page.getByTestId('hold-label')).toHaveText('T')
      await expect(page.getByTestId('next-queue')).toHaveText('JSOZI')
      await expect(color(page, 1, 3)).toHaveAttribute('data-color', '7')
    })

    test('New Game resets the board and stats after some play', async ({ page }) => {
      await page.keyboard.press('ArrowLeft')
      await page.keyboard.press('ArrowUp')
      await page.keyboard.press('Space')
      await expect(page.getByTestId('score-value')).toHaveText('34')
      await page.getByTestId('restart').click()
      await expect(page.getByTestId('score-value')).toHaveText('0')
      await expect(page.getByTestId('next-queue')).toHaveText('LJSOZ')
      await expect(color(page, 0, 4)).toHaveAttribute('data-color', '3')
    })
  })
}
