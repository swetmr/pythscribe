import { Link } from 'react-router'
import { CourseraApp } from '../../../../shared/coursera/CourseraApp.ps'

// Coursera clone — production track. Mounts the shared .ps island, loaded
// LIVE by vite-plugin-pyths (no precompile step). Do NOT edit App.tsx.
export default function Coursera() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>Coursera (PythScribe, production)</h1>
      <CourseraApp />
    </div>
  )
}
