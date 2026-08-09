import { expect } from '@playwright/test'
import type { InteractionScript } from './types'

// Coursera — combined search + category filtering, detail navigation,
// accordion, enroll/progress, quiz validation + scoring + retake, and the
// DELIBERATE error-boundary crash (quiz-crash-dev). The crash step throws
// the SAME Tier-7 custom exception on both tracks (QuizCrashError, surfacing
// as `QuizCrashError: quiz dev crash` from .tsx and .ps/.psc alike) and
// React logs it via console.error on both — covered by the NAME-ANCHORED
// coursera allowlist entry in lib/runtime-capture.ts, so the error
// DIFFERENTIAL stays empty while the boundary behavior itself is diffed and
// any error-naming divergence between tracks still fails.

export const courseraScript: InteractionScript = {
  clone: 'coursera',
  ready: async (page) => {
    await expect(page.getByTestId('coursera-app')).toBeVisible()
  },
  steps: [
    {
      name: 'category chip filters to Computer Science',
      run: (p) => p.getByTestId('chip-computer-science').click(),
      settle: async (p) => {
        await expect(p.getByTestId('catalog-count')).toHaveText('3 courses')
      },
    },
    {
      name: 'search narrows within the active category',
      run: (p) => p.getByTestId('search-input').fill('python'),
      settle: async (p) => {
        await expect(p.getByTestId('catalog-count')).toHaveText('1 courses')
      },
    },
    {
      name: 'switching to All keeps the search',
      run: (p) => p.getByTestId('chip-all').click(),
      settle: async (p) => {
        await expect(p.getByTestId('catalog-count')).toHaveText('2 courses')
      },
    },
    {
      name: 'clearing the search restores the catalog',
      run: (p) => p.getByTestId('search-input').fill(''),
      settle: async (p) => {
        await expect(p.getByTestId('catalog-count')).toHaveText('12 courses')
      },
    },
    {
      name: 'open the compilers course detail',
      run: (p) => p.getByTestId('course-card-cs-compilers').click(),
      settle: async (p) => {
        await expect(p.getByTestId('detail-title')).toHaveText('Compilers: Principles and Practice')
      },
    },
    {
      name: 'accordion opens week 0',
      run: (p) => p.getByTestId('week-toggle-0').click(),
      settle: async (p) => {
        await expect(p.getByTestId('week-lessons-0')).toBeVisible()
      },
    },
    {
      name: 'enroll shows the banner',
      run: (p) => p.getByTestId('enroll-btn').click(),
      settle: async (p) => {
        await expect(p.getByTestId('enrolled-banner')).toContainText('You are enrolled')
      },
    },
    {
      name: 'completing module 0 advances the progress bar',
      run: (p) => p.getByTestId('week-complete-0').click(),
      settle: async (p) => {
        await expect(p.getByTestId('progress-label')).toHaveText('1/3 modules completed')
      },
    },
    {
      name: 'back to the catalog',
      run: (p) => p.getByTestId('back-to-catalog').click(),
      settle: async (p) => {
        await expect(p.getByTestId('catalog-count')).toHaveText('12 courses')
      },
    },
    {
      name: 'open the ML course (has the quiz)',
      run: (p) => p.getByTestId('course-card-py-ml').click(),
      settle: async (p) => {
        await expect(p.getByTestId('quiz')).toBeVisible()
      },
    },
    {
      name: 'empty quiz submit is blocked with per-question errors',
      run: (p) => p.getByTestId('quiz-submit').click(),
      settle: async (p) => {
        await expect(p.getByTestId('quiz-error')).toBeVisible()
        await expect(p.getByTestId('quiz-q-err-q1')).toBeVisible()
      },
    },
    {
      name: 'answer the quiz (q1 wrong) and submit — scores 3/4',
      run: async (p) => {
        await p.getByLabel('Rust').check()
        await p.getByLabel('createElement calls').check()
        await p.getByLabel('useState').check()
        await p.getByLabel('useEffect').check()
        await p.getByTestId('quiz-text-q4').fill('  H1 ')
        await p.getByTestId('quiz-submit').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('quiz-score-line')).toHaveText('You scored 3/4')
        await expect(p.getByTestId('quiz-review-q1')).toHaveAttribute('data-correct', 'false')
      },
    },
    {
      name: 'retake resets the quiz',
      run: (p) => p.getByTestId('quiz-retake').click(),
      settle: async (p) => {
        await expect(p.getByTestId('quiz')).toBeVisible()
        await expect(p.getByTestId('quiz-text-q4')).toHaveValue('')
      },
    },
    {
      name: 'dev crash — the class-component boundary catches the throw',
      run: (p) => p.getByTestId('quiz-crash-dev').click(),
      settle: async (p) => {
        await expect(p.getByTestId('quiz-fallback')).toContainText('The quiz crashed.')
        await expect(p.getByTestId('detail-title')).toBeVisible()
      },
    },
    {
      name: 'reload remounts a fresh quiz',
      run: (p) => p.getByTestId('quiz-reload').click(),
      settle: async (p) => {
        await expect(p.getByTestId('quiz')).toBeVisible()
        await expect(p.getByTestId('quiz-fallback')).toHaveCount(0)
      },
    },
  ],
}
