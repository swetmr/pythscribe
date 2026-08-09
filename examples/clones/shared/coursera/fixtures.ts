// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md).
// Consumed by all three tracks: CourseraApp.tsx imports it the normal ESM way,
// CourseraApp.ps/.psc via `from .fixtures import COURSES, QUIZ` (compiles to a
// plain `./fixtures` ESM import that Vite / Turbopack resolve to this .ts file).

export interface CourseWeek {
  title: string
  lessons: string[]
}

export interface Course {
  id: string
  title: string
  category: string
  instructor: string
  description: string
  weeks: CourseWeek[]
}

export interface QuizQuestion {
  id: string
  kind: 'radio' | 'checkbox' | 'text'
  prompt: string
  options: string[]
  /** radio/text: string; checkbox: string[] (order-insensitive) */
  answer: string | string[]
}

function wk(title: string, lessons: string[]): CourseWeek {
  return { title, lessons }
}

export const COURSES: Course[] = [
  {
    id: 'py-ml',
    title: 'Machine Learning with Python',
    category: 'Data Science',
    instructor: 'Ada Vega',
    description: 'Build and evaluate classic ML models end to end using Python.',
    weeks: [
      wk('Foundations', ['What is ML?', 'Setting up Python', 'NumPy refresher']),
      wk('Supervised learning', ['Linear regression', 'Classification', 'Model metrics']),
      wk('Unsupervised learning', ['Clustering', 'Dimensionality reduction']),
      wk('Capstone', ['Project brief', 'Peer review']),
    ],
  },
  {
    id: 'ds-stats',
    title: 'Statistics for Data Science',
    category: 'Data Science',
    instructor: 'Ravi Menon',
    description: 'Probability, distributions and inference for practical analysis.',
    weeks: [
      wk('Probability basics', ['Sample spaces', 'Conditional probability']),
      wk('Distributions', ['Normal distribution', 'Sampling']),
      wk('Inference', ['Confidence intervals', 'Hypothesis testing']),
    ],
  },
  {
    id: 'ds-viz',
    title: 'Data Visualization Essentials',
    category: 'Data Science',
    instructor: 'Mei Lin',
    description: 'Turn raw tables into honest, readable charts.',
    weeks: [
      wk('Perception', ['Pre-attentive attributes', 'Color pitfalls']),
      wk('Chart forms', ['Bars and lines', 'Distributions', 'Small multiples']),
      wk('Dashboards', ['Layout', 'Interactivity']),
    ],
  },
  {
    id: 'cs-algo',
    title: 'Algorithms and Data Structures',
    category: 'Computer Science',
    instructor: 'Grace Okafor',
    description: 'The classic toolbox: lists, trees, graphs, sorting and search.',
    weeks: [
      wk('Complexity', ['Big-O', 'Amortized analysis']),
      wk('Core structures', ['Stacks and queues', 'Hash maps', 'Trees']),
      wk('Graphs', ['BFS and DFS', 'Shortest paths']),
      wk('Sorting', ['Mergesort', 'Quicksort']),
    ],
  },
  {
    id: 'cs-compilers',
    title: 'Compilers: Principles and Practice',
    category: 'Computer Science',
    instructor: 'Tomás Rivera',
    description: 'Lexing, parsing and code generation for a small language.',
    weeks: [
      wk('Lexing', ['Tokens', 'Finite automata']),
      wk('Parsing', ['Grammars', 'Recursive descent']),
      wk('Codegen', ['IR design', 'Emitting JavaScript']),
    ],
  },
  {
    id: 'cs-python',
    title: 'Python Programming Fundamentals',
    category: 'Computer Science',
    instructor: 'Ada Vega',
    description: 'From first print statement to classes and comprehensions.',
    weeks: [
      wk('Getting started', ['Variables', 'Control flow']),
      wk('Collections', ['Lists and dicts', 'Comprehensions']),
      wk('Functions and classes', ['Closures', 'Dataclasses']),
    ],
  },
  {
    id: 'biz-fin',
    title: 'Corporate Finance Basics',
    category: 'Business',
    instructor: 'Lena Fischer',
    description: 'Time value of money, budgeting and capital decisions.',
    weeks: [
      wk('Time value of money', ['Discounting', 'NPV and IRR']),
      wk('Budgeting', ['Cash flow forecasting', 'Working capital']),
      wk('Capital structure', ['Debt vs equity']),
    ],
  },
  {
    id: 'biz-mkt',
    title: 'Digital Marketing Strategy',
    category: 'Business',
    instructor: 'Diego Santos',
    description: 'Channels, funnels and measurement for modern marketing.',
    weeks: [
      wk('Foundations', ['Positioning', 'Personas']),
      wk('Channels', ['Search', 'Social', 'Email']),
      wk('Measurement', ['Attribution', 'Experiments']),
    ],
  },
  {
    id: 'art-photo',
    title: 'Photography Composition',
    category: 'Arts',
    instructor: 'Noor Haddad',
    description: 'See like a camera: framing, light and story.',
    weeks: [
      wk('Framing', ['Rule of thirds', 'Leading lines']),
      wk('Light', ['Golden hour', 'Hard vs soft light']),
      wk('Story', ['Series', 'Editing a set']),
    ],
  },
  {
    id: 'art-music',
    title: 'Music Theory 101',
    category: 'Arts',
    instructor: 'Jonas Berg',
    description: 'Scales, chords and rhythm from the ground up.',
    weeks: [
      wk('Pitch', ['Notes and scales', 'Intervals']),
      wk('Harmony', ['Triads', 'Chord progressions']),
      wk('Rhythm', ['Meter', 'Syncopation']),
    ],
  },
  {
    id: 'health-nutrition',
    title: 'Nutrition Science',
    category: 'Health',
    instructor: 'Sara Kim',
    description: 'Macronutrients, metabolism and evidence-based eating.',
    weeks: [
      wk('Macronutrients', ['Carbs, fat, protein', 'Energy balance']),
      wk('Micronutrients', ['Vitamins', 'Minerals']),
      wk('Applied nutrition', ['Reading studies', 'Meal planning']),
    ],
  },
  {
    id: 'health-anatomy',
    title: 'Human Anatomy',
    category: 'Health',
    instructor: 'Omar Farouk',
    description: 'A systems tour of the human body.',
    weeks: [
      wk('Skeletal system', ['Bones', 'Joints']),
      wk('Muscular system', ['Muscle groups', 'Movement']),
      wk('Nervous system', ['Neurons', 'Reflexes']),
    ],
  },
]

export const QUIZ = {
  title: 'Graded quiz',
  questions: [
    {
      id: 'q1',
      kind: 'radio',
      prompt: 'Which language does PythScribe compile to?',
      options: ['Rust', 'JavaScript', 'C++', 'Go'],
      answer: 'JavaScript',
    },
    {
      id: 'q2',
      kind: 'radio',
      prompt: 'What does JSX compile down to?',
      options: ['createElement calls', 'HTML strings', 'Web Components', 'Template literals'],
      answer: 'createElement calls',
    },
    {
      id: 'q3',
      kind: 'checkbox',
      prompt: 'Which of these are real React hooks? (select all that apply)',
      options: ['useState', 'useEffect', 'useQuery', 'useClass'],
      answer: ['useState', 'useEffect'],
    },
    {
      id: 'q4',
      kind: 'text',
      prompt: 'Which HTML tag renders a top-level heading? (lowercase, no brackets)',
      options: [],
      answer: 'h1',
    },
  ] as QuizQuestion[],
}
