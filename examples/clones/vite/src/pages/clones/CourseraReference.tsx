import { Link } from 'react-router'
import CourseraApp from '../../../../shared/coursera/CourseraApp'

// Coursera clone — React reference oracle mirror (dev/benchmark track).
export default function CourseraReference() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <h1>Coursera (React reference)</h1>
      <CourseraApp />
    </div>
  )
}
