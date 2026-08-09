import { Link } from 'react-router'
import { CourseraApp } from '../../../../shared/coursera/CourseraApp.psc'

// Coursera clone — .psc (compressed) track. Mounts the COMPRESSED
// CourseraApp.psc (vite-plugin-pyths expands + compiles live). Renders
// identically to the .ps track by the Iron Rule.
export default function CourseraPsc() {
  return (
    <div className="shell">
      {/* No extra "· .psc track" banner here: the interaction differential's
          PYTHS_E2E_PSC lockstep mode byte-diffs this shell against the oracle
          page (dom-snapshot.ts normalizes the <h1> track label, which already
          identifies the track, but not arbitrary banner chrome). */}
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>Coursera (PythScribe, .psc compressed)</h1>
      <CourseraApp />
    </div>
  )
}
