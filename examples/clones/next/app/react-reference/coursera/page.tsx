/**
 * Coursera clone oracle page (React reference track).
 * Renders the CourseraApp.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/coursera/page.ps.
 */
import CourseraApp from '../../../../shared/coursera/CourseraApp'

export default function Page() {
  return (
    <main style={{ padding: 20 }}>
      <p>
        <a href="/">&larr; home</a>
      </p>
      <h1>Coursera (React reference)</h1>
      <CourseraApp />
    </main>
  )
}
