// Tri-track render-parity for the Coursera clone (see CONTRIBUTING.md).
// Mounts all 3 tracks (tsx oracle / .ps canonical / .psc compressed) with the
// same fixtures and asserts (a) each track satisfies the behavioral contract,
// (b) DOM-snapshot equality between the oracle and both PythScribe tracks at
// initial render and after state-changing interactions (search+filter, detail
// accordion/enroll/progress, quiz validation, quiz scoring, boundary crash).

import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { UserEvent } from '@testing-library/user-event'
import ReactCourseraApp, { QuizCrashError as TsxQuizCrashError } from './CourseraApp'
import { CourseraApp as PsCourseraApp, QuizCrashError as PsQuizCrashError } from './CourseraApp.ps'
import { CourseraApp as PscCourseraApp, QuizCrashError as PscQuizCrashError } from './CourseraApp.psc'
import { COURSES, QUIZ } from './fixtures'

type AppComponent = React.ComponentType<Record<string, never>>

// React logs caught boundary errors via console.error — silence them for the
// crash-path tests so the output stays readable. Restored after each test.
function muteConsoleError() {
  return vi.spyOn(console, 'error').mockImplementation(() => {})
}

afterEach(() => {
  vi.restoreAllMocks()
})

async function openDetail(user: UserEvent, courseId: string) {
  await user.click(screen.getByTestId('course-card-' + courseId))
}

// Known answer set: q1 deliberately WRONG, q2/q3/q4 correct → score 3/4.
async function answerKnownSet(user: UserEvent) {
  await user.click(screen.getByLabelText('Rust')) // q1 — wrong (correct: JavaScript)
  await user.click(screen.getByLabelText('createElement calls')) // q2 — correct
  await user.click(screen.getByLabelText('useState')) // q3 — correct (multi)
  await user.click(screen.getByLabelText('useEffect'))
  await user.type(screen.getByTestId('quiz-text-q4'), '  H1 ') // q4 — correct after trim+lower
}

function contract(label: string, App: AppComponent) {
  describe(label, () => {
    it('renders the full catalog from fixtures', () => {
      render(<App />)
      expect(screen.getByTestId('catalog')).toBeInTheDocument()
      expect(screen.getByTestId('catalog-count')).toHaveTextContent(
        String(COURSES.length) + ' courses',
      )
      for (const c of COURSES) {
        expect(screen.getByTestId('course-card-' + c.id)).toBeInTheDocument()
      }
    })

    it('text search narrows the catalog', async () => {
      const user = userEvent.setup()
      render(<App />)
      await user.type(screen.getByTestId('search-input'), 'python')
      expect(screen.getByTestId('catalog-count')).toHaveTextContent('2 courses')
      expect(screen.getByTestId('course-card-py-ml')).toBeInTheDocument()
      expect(screen.getByTestId('course-card-cs-python')).toBeInTheDocument()
      expect(screen.queryByTestId('course-card-biz-fin')).not.toBeInTheDocument()
    })

    it('category chips filter the catalog and search + chip combine', async () => {
      const user = userEvent.setup()
      render(<App />)
      await user.click(screen.getByTestId('chip-computer-science'))
      expect(screen.getByTestId('chip-computer-science')).toHaveAttribute('data-active', 'true')
      expect(screen.getByTestId('catalog-count')).toHaveTextContent('3 courses')
      // combine: search narrows within the active category
      await user.type(screen.getByTestId('search-input'), 'python')
      expect(screen.getByTestId('catalog-count')).toHaveTextContent('1 courses')
      expect(screen.getByTestId('course-card-cs-python')).toBeInTheDocument()
      expect(screen.queryByTestId('course-card-py-ml')).not.toBeInTheDocument()
      // no match → empty state
      await user.clear(screen.getByTestId('search-input'))
      await user.type(screen.getByTestId('search-input'), 'zzz')
      expect(screen.getByTestId('catalog-empty')).toBeInTheDocument()
    })

    it('opens course detail and navigates back', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'cs-compilers')
      expect(screen.getByTestId('detail-title')).toHaveTextContent(
        'Compilers: Principles and Practice',
      )
      await user.click(screen.getByTestId('back-to-catalog'))
      expect(screen.getByTestId('catalog')).toBeInTheDocument()
    })

    it('syllabus accordion expands and collapses weeks', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'cs-compilers')
      expect(screen.queryByTestId('week-lessons-0')).not.toBeInTheDocument()
      await user.click(screen.getByTestId('week-toggle-0'))
      expect(screen.getByTestId('week-toggle-0')).toHaveAttribute('data-open', 'true')
      expect(screen.getByTestId('week-lessons-0')).toHaveTextContent('Tokens')
      await user.click(screen.getByTestId('week-toggle-0'))
      expect(screen.queryByTestId('week-lessons-0')).not.toBeInTheDocument()
    })

    it('enroll button flips to the enrolled banner', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'py-ml')
      expect(screen.queryByTestId('enrolled-banner')).not.toBeInTheDocument()
      await user.click(screen.getByTestId('enroll-btn'))
      expect(screen.getByTestId('enrolled-banner')).toHaveTextContent(
        'You are enrolled in this course.',
      )
      expect(screen.queryByTestId('enroll-btn')).not.toBeInTheDocument()
    })

    it('progress bar tracks modules completed', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'cs-compilers') // 3 weeks
      expect(screen.getByTestId('progress-label')).toHaveTextContent('0/3 modules completed')
      expect(screen.getByTestId('progress-fill')).toHaveStyle({ width: '0%' })
      await user.click(screen.getByTestId('week-complete-0'))
      expect(screen.getByTestId('progress-label')).toHaveTextContent('1/3 modules completed')
      expect(screen.getByTestId('progress-fill')).toHaveStyle({ width: '33%' })
      await user.click(screen.getByTestId('week-complete-1'))
      expect(screen.getByTestId('progress-label')).toHaveTextContent('2/3 modules completed')
      // toggling off works too
      await user.click(screen.getByTestId('week-complete-0'))
      expect(screen.getByTestId('progress-label')).toHaveTextContent('1/3 modules completed')
    })

    it('quiz validation blocks an empty submit with per-question errors', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'py-ml')
      await user.click(screen.getByTestId('quiz-submit'))
      expect(screen.getByTestId('quiz-error')).toBeInTheDocument()
      for (const q of QUIZ.questions) {
        expect(screen.getByTestId('quiz-q-err-' + q.id)).toBeInTheDocument()
      }
      expect(screen.queryByTestId('quiz-score')).not.toBeInTheDocument()
      // answering one question clears only that error on next submit
      await user.click(screen.getByLabelText('JavaScript'))
      await user.click(screen.getByTestId('quiz-submit'))
      expect(screen.queryByTestId('quiz-q-err-q1')).not.toBeInTheDocument()
      expect(screen.getByTestId('quiz-q-err-q2')).toBeInTheDocument()
    })

    it('scores a known answer set 3/4 with per-question review and retake', async () => {
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'py-ml')
      await answerKnownSet(user)
      await user.click(screen.getByTestId('quiz-submit'))
      expect(screen.getByTestId('quiz-score-line')).toHaveTextContent('You scored 3/4')
      expect(screen.getByTestId('quiz-review-q1')).toHaveAttribute('data-correct', 'false')
      expect(screen.getByTestId('quiz-review-q2')).toHaveAttribute('data-correct', 'true')
      expect(screen.getByTestId('quiz-review-q3')).toHaveAttribute('data-correct', 'true')
      expect(screen.getByTestId('quiz-review-q4')).toHaveAttribute('data-correct', 'true')
      await user.click(screen.getByTestId('quiz-retake'))
      expect(screen.getByTestId('quiz')).toBeInTheDocument()
      expect(screen.getByTestId('quiz-text-q4')).toHaveValue('')
    })

    it('QuizBoundary (class component) catches the dev-trigger throw and reloads', async () => {
      muteConsoleError()
      const user = userEvent.setup()
      render(<App />)
      await openDetail(user, 'py-ml')
      expect(screen.getByTestId('quiz')).toBeInTheDocument()
      await user.click(screen.getByTestId('quiz-crash-dev'))
      expect(screen.getByTestId('quiz-fallback')).toHaveTextContent('The quiz crashed.')
      expect(screen.queryByTestId('quiz')).not.toBeInTheDocument()
      // the syllabus outside the boundary survives the crash
      expect(screen.getByTestId('detail-title')).toBeInTheDocument()
      await user.click(screen.getByTestId('quiz-reload'))
      expect(screen.getByTestId('quiz')).toBeInTheDocument()
      expect(screen.queryByTestId('quiz-fallback')).not.toBeInTheDocument()
    })
  })
}

contract('CourseraApp.tsx (React reference)', ReactCourseraApp)
contract('CourseraApp.ps (PythScribe canonical)', PsCourseraApp)
contract('CourseraApp.psc (compressed PythScribe)', PscCourseraApp)

describe('Tri-track DOM parity', () => {
  async function snapshot(
    App: AppComponent,
    flow: (user: UserEvent) => Promise<void>,
  ): Promise<string> {
    const user = userEvent.setup()
    const { container, unmount } = render(<App />)
    await flow(user)
    const html = container.innerHTML
    unmount()
    return html
  }

  async function assertParity(flow: (user: UserEvent) => Promise<void>) {
    const r = await snapshot(ReactCourseraApp, flow)
    const p = await snapshot(PsCourseraApp, flow)
    const c = await snapshot(PscCourseraApp, flow)
    expect(p).toBe(r)
    expect(c).toBe(r)
  }

  it('initial catalog DOM matches', async () => {
    await assertParity(async () => {})
  })

  it('searched + chip-filtered catalog DOM matches', async () => {
    await assertParity(async (user) => {
      await user.click(screen.getByTestId('chip-data-science'))
      await user.type(screen.getByTestId('search-input'), 'data')
    })
  })

  it('course detail DOM matches after accordion + enroll + progress', async () => {
    await assertParity(async (user) => {
      await openDetail(user, 'cs-compilers')
      await user.click(screen.getByTestId('week-toggle-1'))
      await user.click(screen.getByTestId('enroll-btn'))
      await user.click(screen.getByTestId('week-complete-0'))
    })
  })

  it('quiz empty-submit validation DOM matches', async () => {
    await assertParity(async (user) => {
      await openDetail(user, 'py-ml')
      await user.click(screen.getByTestId('quiz-submit'))
    })
  })

  it('quiz score screen DOM matches for the known answer set', async () => {
    await assertParity(async (user) => {
      await openDetail(user, 'py-ml')
      await answerKnownSet(user)
      await user.click(screen.getByTestId('quiz-submit'))
    })
  })

  it('crashed-quiz boundary fallback DOM matches', async () => {
    muteConsoleError()
    await assertParity(async (user) => {
      await openDetail(user, 'py-ml')
      await user.click(screen.getByTestId('quiz-crash-dev'))
    })
  })
})

describe('Tier-7 dev-crash error identity (tri-track)', () => {
  // Regression pin for the Exception-vs-Error naming divergence: the .ps
  // twin used to `raise Exception(...)` (surfacing "Exception: quiz dev
  // crash") while the .tsx oracle threw a bare Error ("Error: quiz dev
  // crash"), and the e2e interaction differential absorbed the mismatch
  // with a message-substring allowlist entry. All three tracks now throw
  // the SAME Tier-7 custom exception class, so the browser channels
  // (pageerror / console.error via React 19 onCaughtError) surface an
  // IDENTICAL first line — which is what lets e2e/lib/runtime-capture.ts
  // anchor its coursera allowlist entry on the exact error name.
  const CASES: [string, new (message: string) => Error][] = [
    ['tsx oracle', TsxQuizCrashError],
    ['.ps canonical', PsQuizCrashError],
    ['.psc compressed', PscQuizCrashError],
  ]

  it.each(CASES)('%s surfaces "QuizCrashError: quiz dev crash"', (_label, Cls) => {
    const err = new Cls('quiz dev crash')
    expect(err).toBeInstanceOf(Error) // boundary/pageerror contract
    expect(`${err.name}: ${err.message}`).toBe('QuizCrashError: quiz dev crash')
    // pageerror normalization compares the first stack line — pin it too
    expect(String(err.stack || '').split('\n')[0]).toBe('QuizCrashError: quiz dev crash')
  })
})
