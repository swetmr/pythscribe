'use client'
import { Component, ReactNode, useState } from 'react'
import './CourseraApp.css'
import { COURSES, QUIZ, Course, QuizQuestion } from './fixtures'

/**
 * React reference oracle for the Coursera clone. Dual-track-paired with
 * CourseraApp.ps / CourseraApp.psc — all three tracks must render identical
 * DOM for the same fixtures (see CourseraApp.test.tsx).
 *
 * Components: CourseraApp (view switch) → Catalog (search + category chips),
 * CourseDetail (syllabus accordion, enroll, progress) → QuizBoundary (class
 * error boundary) → Quiz (graded radio/checkbox/text quiz with validation,
 * scoring, retake, and a hidden dev-only crash trigger for the boundary e2e).
 */

function slug(s: string): string {
  return s.toLowerCase().replace(/ /g, '-')
}

/**
 * Tier-7 crash error — dual-tracked with `class QuizCrashError(Exception)`
 * in CourseraApp.ps/.psc. Both tracks surface the deliberate dev crash
 * identically as `QuizCrashError: quiz dev crash` (the PythScribe runtime
 * stamps `.name` from the Python class name), so the e2e interaction
 * differential enforces exact error identity — name + message — instead of
 * allowlisting a name-blind message substring.
 */
export class QuizCrashError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'QuizCrashError'
  }
}

export function Catalog({ onSelect }: { onSelect: (id: string) => void }) {
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState('All')
  const cats = ['All']
  for (const c of COURSES) {
    if (!cats.includes(c.category)) cats.push(c.category)
  }
  const q = query.trim().toLowerCase()
  const filtered = COURSES.filter(
    (c) =>
      (q === '' || c.title.toLowerCase().includes(q)) &&
      (category === 'All' || c.category === category),
  )
  return (
    <section className="cx-catalog" data-testid="catalog">
      <h1>Explore courses</h1>
      <input
        className="cx-search"
        data-testid="search-input"
        type="text"
        placeholder="Search courses"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="cx-chips">
        {cats.map((cat) => (
          <button
            key={cat}
            className="cx-chip"
            data-testid={'chip-' + slug(cat)}
            data-active={cat === category}
            onClick={() => setCategory(cat)}
          >
            {cat}
          </button>
        ))}
      </div>
      <p className="cx-count" data-testid="catalog-count">
        {String(filtered.length) + ' courses'}
      </p>
      {filtered.length === 0 && (
        <p className="cx-empty" data-testid="catalog-empty">
          No courses match your filters.
        </p>
      )}
      <div className="cx-grid">
        {filtered.map((c) => (
          <article
            key={c.id}
            className="cx-card"
            data-testid={'course-card-' + c.id}
            onClick={() => onSelect(c.id)}
          >
            <span className="cx-cat-tag">{c.category}</span>
            <h2>{c.title}</h2>
            <p className="cx-inst">{c.instructor}</p>
          </article>
        ))}
      </div>
    </section>
  )
}

export function Quiz() {
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({})
  const [errors, setErrors] = useState<string[]>([])
  const [submitted, setSubmitted] = useState(false)
  const [crashed, setCrashed] = useState(false)
  if (crashed) {
    throw new QuizCrashError('quiz dev crash')
  }

  const val = (qid: string) => (qid in answers ? answers[qid] : null)

  const setRadio = (qid: string, opt: string) => {
    setAnswers({ ...answers, [qid]: opt })
  }

  const toggleCheck = (qid: string, opt: string) => {
    const cur = (val(qid) as string[] | null) ?? []
    const nxt = cur.includes(opt) ? cur.filter((o) => o !== opt) : cur.concat([opt])
    setAnswers({ ...answers, [qid]: nxt })
  }

  const setText = (qid: string, text: string) => {
    setAnswers({ ...answers, [qid]: text })
  }

  const answered = (q: QuizQuestion) => {
    const v = val(q.id)
    if (q.kind === 'checkbox') return v !== null && (v as string[]).length > 0
    if (q.kind === 'text') return v !== null && (v as string).trim() !== ''
    return v !== null
  }

  const sameSet = (a: string[], b: string[]) => {
    if (a.length !== b.length) return false
    for (const x of a) {
      if (!b.includes(x)) return false
    }
    return true
  }

  const correct = (q: QuizQuestion) => {
    const v = val(q.id)
    if (q.kind === 'checkbox') return sameSet((v as string[] | null) ?? [], q.answer as string[])
    if (q.kind === 'text')
      return (((v as string | null) ?? '').trim().toLowerCase()) === (q.answer as string).toLowerCase()
    return v === q.answer
  }

  const submit = () => {
    const missing = QUIZ.questions.filter((q) => !answered(q)).map((q) => q.id)
    setErrors(missing)
    if (missing.length === 0) setSubmitted(true)
  }

  const retake = () => {
    setAnswers({})
    setErrors([])
    setSubmitted(false)
  }

  if (submitted) {
    const score = QUIZ.questions.filter((q) => correct(q)).length
    return (
      <div className="cx-quiz" data-testid="quiz-score">
        <h3 data-testid="quiz-score-line">
          {'You scored ' + String(score) + '/' + String(QUIZ.questions.length)}
        </h3>
        <ul className="cx-review">
          {QUIZ.questions.map((q) => (
            <li key={q.id} data-testid={'quiz-review-' + q.id} data-correct={correct(q)}>
              {(correct(q) ? 'Correct: ' : 'Incorrect: ') + q.prompt}
            </li>
          ))}
        </ul>
        <button className="cx-btn" data-testid="quiz-retake" onClick={retake}>
          Retake quiz
        </button>
      </div>
    )
  }

  return (
    <div className="cx-quiz" data-testid="quiz">
      {QUIZ.questions.map((q, idx) => (
        <fieldset key={q.id} className="cx-q" data-testid={'quiz-q-' + q.id}>
          <legend>{String(idx + 1) + '. ' + q.prompt}</legend>
          {q.kind === 'radio' &&
            q.options.map((opt) => (
              <label key={opt} className="cx-opt">
                <input
                  type="radio"
                  name={q.id}
                  value={opt}
                  checked={val(q.id) === opt}
                  onChange={() => setRadio(q.id, opt)}
                />
                {opt}
              </label>
            ))}
          {q.kind === 'checkbox' &&
            q.options.map((opt) => (
              <label key={opt} className="cx-opt">
                <input
                  type="checkbox"
                  name={q.id}
                  value={opt}
                  checked={val(q.id) !== null && (val(q.id) as string[]).includes(opt)}
                  onChange={() => toggleCheck(q.id, opt)}
                />
                {opt}
              </label>
            ))}
          {q.kind === 'text' && (
            <input
              className="cx-text-answer"
              data-testid={'quiz-text-' + q.id}
              type="text"
              placeholder="Your answer"
              value={(val(q.id) as string | null) ?? ''}
              onChange={(e) => setText(q.id, e.target.value)}
            />
          )}
          {errors.includes(q.id) && (
            <p className="cx-q-err" data-testid={'quiz-q-err-' + q.id}>
              Please answer this question.
            </p>
          )}
        </fieldset>
      ))}
      {errors.length > 0 && (
        <p className="cx-quiz-err" data-testid="quiz-error">
          Answer all questions before submitting.
        </p>
      )}
      <div className="cx-quiz-actions">
        <button className="cx-btn" data-testid="quiz-submit" onClick={submit}>
          Submit quiz
        </button>
        <button
          className="cx-devcrash"
          data-testid="quiz-crash-dev"
          aria-hidden="true"
          tabIndex={-1}
          onClick={() => setCrashed(true)}
        >
          dev: crash quiz
        </button>
      </div>
    </div>
  )
}

export class QuizBoundary extends Component<{ children?: ReactNode }, { hasError: boolean }> {
  constructor(props: { children?: ReactNode }) {
    super(props)
    this.state = { hasError: false }
  }

  static getDerivedStateFromError(_error: unknown) {
    return { hasError: true }
  }

  componentDidCatch(_error: unknown, _info: unknown) {}

  render() {
    if (this.state.hasError) {
      return (
        <div className="cx-quiz-fallback" data-testid="quiz-fallback">
          <p>The quiz crashed.</p>
          <button
            className="cx-btn"
            data-testid="quiz-reload"
            onClick={() => this.setState({ hasError: false })}
          >
            Reload quiz
          </button>
        </div>
      )
    }
    return this.props.children
  }
}

export function CourseDetail({ course, onBack }: { course: Course; onBack: () => void }) {
  const [enrolled, setEnrolled] = useState(false)
  const [openWeeks, setOpenWeeks] = useState<number[]>([])
  const [completed, setCompleted] = useState<number[]>([])
  const total = course.weeks.length
  const pct = Math.floor((completed.length * 100) / total)

  const toggleWeek = (i: number) => {
    setOpenWeeks(openWeeks.includes(i) ? openWeeks.filter((w) => w !== i) : openWeeks.concat([i]))
  }

  const toggleComplete = (i: number) => {
    setCompleted(completed.includes(i) ? completed.filter((w) => w !== i) : completed.concat([i]))
  }

  return (
    <section className="cx-detail" data-testid="course-detail">
      <button className="cx-back" data-testid="back-to-catalog" onClick={onBack}>
        All courses
      </button>
      <span className="cx-cat-tag">{course.category}</span>
      <h1 data-testid="detail-title">{course.title}</h1>
      <p className="cx-inst">{'Taught by ' + course.instructor}</p>
      <p className="cx-desc">{course.description}</p>
      {enrolled ? (
        <div className="cx-banner" data-testid="enrolled-banner">
          You are enrolled in this course.
        </div>
      ) : (
        <button className="cx-enroll" data-testid="enroll-btn" onClick={() => setEnrolled(true)}>
          Enroll for free
        </button>
      )}
      <div className="cx-progress">
        <div className="cx-progress-track">
          <div className="cx-progress-fill" data-testid="progress-fill" style={{ width: String(pct) + '%' }} />
        </div>
        <span data-testid="progress-label">
          {String(completed.length) + '/' + String(total) + ' modules completed'}
        </span>
      </div>
      <h2>Syllabus</h2>
      <div className="cx-weeks">
        {course.weeks.map((w, i) => (
          <div key={String(i)} className="cx-week" data-testid={'week-' + String(i)}>
            <div className="cx-week-head">
              <button
                className="cx-week-toggle"
                data-testid={'week-toggle-' + String(i)}
                data-open={openWeeks.includes(i)}
                onClick={() => toggleWeek(i)}
              >
                {'Week ' + String(i + 1) + ': ' + w.title}
              </button>
              <button
                className="cx-week-done"
                data-testid={'week-complete-' + String(i)}
                data-done={completed.includes(i)}
                onClick={() => toggleComplete(i)}
              >
                {completed.includes(i) ? 'Completed' : 'Mark complete'}
              </button>
            </div>
            {openWeeks.includes(i) && (
              <ul className="cx-lessons" data-testid={'week-lessons-' + String(i)}>
                {w.lessons.map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </div>
      <h2>{QUIZ.title}</h2>
      <QuizBoundary>
        <Quiz />
      </QuizBoundary>
    </section>
  )
}

export function CourseraApp() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const matches = COURSES.filter((c) => c.id === selectedId)
  const course = matches.length > 0 ? matches[0] : null
  return (
    <div className="cx-app" data-testid="coursera-app">
      <header className="cx-header">
        <span className="cx-logo">coursera</span>
        <span className="cx-tagline">clone demo</span>
      </header>
      {course === null ? (
        <Catalog onSelect={setSelectedId} />
      ) : (
        <CourseDetail course={course} onBack={() => setSelectedId(null)} />
      )}
    </div>
  )
}

export default CourseraApp
