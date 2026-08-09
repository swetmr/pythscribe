import { test, expect } from './lib/parity-test'
import type { Page } from '@playwright/test'

// Coursera clone e2e — runs against BOTH apps (playwright.vite.config.ts /
// playwright.next.config.ts differ only in baseURL/webServer). Specs are
// written against routes + shared-component testids only, never against
// either shell's internals, so the same file covers all four mounts:
// /coursera (PythScribe production track) and /react-reference/coursera
// (React oracle track) in each app.

// Oracle first — see the ledger-subtraction note in lib/runtime-capture.ts.
const ROUTES = [
  { label: 'react-reference (oracle)', path: '/react-reference/coursera' },
  { label: 'production (PythScribe)', path: '/coursera' },
]

async function openDetail(page: Page, courseId: string) {
  await page.getByTestId('course-card-' + courseId).click()
  await expect(page.getByTestId('course-detail')).toBeVisible()
}

for (const route of ROUTES) {
  test.describe('Coursera ' + route.label, () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(route.path)
      await expect(page.getByTestId('coursera-app')).toBeVisible()
    })

    test('catalog renders and search + category filter combine', async ({ page }) => {
      await expect(page.getByTestId('catalog-count')).toHaveText('12 courses')

      // category chip alone
      await page.getByTestId('chip-computer-science').click()
      await expect(page.getByTestId('chip-computer-science')).toHaveAttribute(
        'data-active',
        'true',
      )
      await expect(page.getByTestId('catalog-count')).toHaveText('3 courses')

      // + text search narrows WITHIN the active category (combined filtering)
      await page.getByTestId('search-input').fill('python')
      await expect(page.getByTestId('catalog-count')).toHaveText('1 courses')
      await expect(page.getByTestId('course-card-cs-python')).toBeVisible()
      await expect(page.getByTestId('course-card-py-ml')).toHaveCount(0)

      // same search under All shows both python courses
      await page.getByTestId('chip-all').click()
      await expect(page.getByTestId('catalog-count')).toHaveText('2 courses')
      await expect(page.getByTestId('course-card-py-ml')).toBeVisible()

      // no match → empty state
      await page.getByTestId('search-input').fill('zzz-no-match')
      await expect(page.getByTestId('catalog-empty')).toBeVisible()
    })

    test('syllabus accordion toggles and enroll + progress work', async ({ page }) => {
      await openDetail(page, 'cs-compilers')
      await expect(page.getByTestId('detail-title')).toHaveText(
        'Compilers: Principles and Practice',
      )

      // accordion: closed → open → closed
      await expect(page.getByTestId('week-lessons-0')).toHaveCount(0)
      await page.getByTestId('week-toggle-0').click()
      await expect(page.getByTestId('week-lessons-0')).toBeVisible()
      await expect(page.getByTestId('week-lessons-0')).toContainText('Tokens')
      await page.getByTestId('week-toggle-0').click()
      await expect(page.getByTestId('week-lessons-0')).toHaveCount(0)

      // enroll flips to the banner
      await page.getByTestId('enroll-btn').click()
      await expect(page.getByTestId('enrolled-banner')).toContainText('You are enrolled')

      // progress bar tracks completed modules
      await expect(page.getByTestId('progress-label')).toHaveText('0/3 modules completed')
      await page.getByTestId('week-complete-0').click()
      await expect(page.getByTestId('progress-label')).toHaveText('1/3 modules completed')
    })

    test('quiz validation blocks an empty submit', async ({ page }) => {
      await openDetail(page, 'py-ml')
      await page.getByTestId('quiz-submit').click()
      await expect(page.getByTestId('quiz-error')).toBeVisible()
      await expect(page.getByTestId('quiz-q-err-q1')).toBeVisible()
      await expect(page.getByTestId('quiz-q-err-q4')).toBeVisible()
      await expect(page.getByTestId('quiz-score')).toHaveCount(0)
    })

    test('quiz scores a known answer set 3/4 with review + retake', async ({ page }) => {
      await openDetail(page, 'py-ml')
      // q1 deliberately wrong, q2-q4 correct
      await page.getByLabel('Rust').check()
      await page.getByLabel('createElement calls').check()
      await page.getByLabel('useState').check()
      await page.getByLabel('useEffect').check()
      await page.getByTestId('quiz-text-q4').fill('  H1 ')
      await page.getByTestId('quiz-submit').click()

      await expect(page.getByTestId('quiz-score-line')).toHaveText('You scored 3/4')
      await expect(page.getByTestId('quiz-review-q1')).toHaveAttribute('data-correct', 'false')
      await expect(page.getByTestId('quiz-review-q2')).toHaveAttribute('data-correct', 'true')
      await expect(page.getByTestId('quiz-review-q3')).toHaveAttribute('data-correct', 'true')
      await expect(page.getByTestId('quiz-review-q4')).toHaveAttribute('data-correct', 'true')

      await page.getByTestId('quiz-retake').click()
      await expect(page.getByTestId('quiz')).toBeVisible()
      await expect(page.getByTestId('quiz-text-q4')).toHaveValue('')
    })

    test('QuizBoundary class component catches the dev-trigger throw', async ({ page }) => {
      await openDetail(page, 'py-ml')
      await expect(page.getByTestId('quiz')).toBeVisible()
      await page.getByTestId('quiz-crash-dev').click()
      await expect(page.getByTestId('quiz-fallback')).toContainText('The quiz crashed.')
      await expect(page.getByTestId('quiz')).toHaveCount(0)
      // the rest of the detail page survives the crash
      await expect(page.getByTestId('detail-title')).toBeVisible()
      // reload remounts a fresh quiz
      await page.getByTestId('quiz-reload').click()
      await expect(page.getByTestId('quiz')).toBeVisible()
      await expect(page.getByTestId('quiz-fallback')).toHaveCount(0)
    })
  })
}
