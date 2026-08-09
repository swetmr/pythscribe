// Local constants ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). Tetris is a pure deterministic game: the whole piece
// sequence is derived from SEED via a seeded 7-bag (Park–Miller / MINSTD +
// Fisher–Yates), and gravity/lock are driven by a controllable time source
// (setTimeout under Playwright's page.clock), so every track produces
// byte-identical DOM for the same key sequence + clock advances. No
// Math.random, no Date.now — the "fixture" here is the seed + the SRS tables.
//
// These tables (PIECES, the four KICK tables, SCORES) are DATA, not logic, so
// they are shared verbatim by all three tracks (.tsx / .ps / .psc). The game
// LOGIC (bag shuffle, collision, rotation-with-kicks, line clear, scoring,
// the gravity/lock reducer) is re-implemented independently in each track —
// that re-implementation is what the parity suite actually tests.

export type Status = 'playing' | 'over'

export interface GameState {
  board: number[] // flat ROWS*COLS, index = row*COLS + col; 0 = empty, 1..7 = color
  seed: number // current PRNG state (seeded, deterministic)
  queue: number[] // upcoming piece types (>= PREVIEW after every mutation)
  current: number // active piece type (0..6)
  rot: number // active rotation state (0..3)
  pr: number // active piece box top-left row
  pc: number // active piece box top-left col
  hold: number | null // held piece type, or null
  canHold: boolean // one hold per drop
  score: number
  lines: number
  level: number
  status: Status
}

export const COLS = 10
export const ROWS = 20
export const SPAWN_ROW = 0
export const SPAWN_COL = 3
export const PREVIEW = 5 // upcoming piece letters shown in next-queue

// Park–Miller MINSTD seed. Chosen so the first bag + spawn are stable and
// visually varied. Kept < 2^31 so `state * 16807` stays within 2^53 — the
// arithmetic is bit-exact in JS and PythScribe (no bitwise ops / BigInt).
export const SEED = 1337

// Gravity + lock delay in ms. Driven by setTimeout, faked by page.clock in
// e2e (advanced deterministically). Chosen large enough that they NEVER fire
// during the fast, fully-synchronous vitest render-parity tests (which use
// fireEvent and never yield to the macrotask queue), so those snapshots stay
// stable — while the e2e interaction differential advances the clock to fire
// them on purpose. This is the timer/game-loop coverage 2048 lacks.
export const GRAVITY_MS = 1000
export const LOCK_MS = 500

// Piece letters, indexed by type. 0=I 1=O 2=T 3=S 4=Z 5=J 6=L. Color = type+1.
export const LETTERS = ['I', 'O', 'T', 'S', 'Z', 'J', 'L']

// SRS rotation states. PIECES[type][rot] = list of 4 [row, col] minos in the
// piece's local box (I/O in a 4x4 frame, JLSTZ in a 3x3 frame). A mino's
// board cell = [pr + row, pc + col]. These are the canonical Super Rotation
// System spawn orientations.
export const PIECES: number[][][][] = [
  // I
  [
    [[1, 0], [1, 1], [1, 2], [1, 3]],
    [[0, 2], [1, 2], [2, 2], [3, 2]],
    [[2, 0], [2, 1], [2, 2], [2, 3]],
    [[0, 1], [1, 1], [2, 1], [3, 1]],
  ],
  // O (rotation is a no-op)
  [
    [[0, 1], [0, 2], [1, 1], [1, 2]],
    [[0, 1], [0, 2], [1, 1], [1, 2]],
    [[0, 1], [0, 2], [1, 1], [1, 2]],
    [[0, 1], [0, 2], [1, 1], [1, 2]],
  ],
  // T
  [
    [[0, 1], [1, 0], [1, 1], [1, 2]],
    [[0, 1], [1, 1], [1, 2], [2, 1]],
    [[1, 0], [1, 1], [1, 2], [2, 1]],
    [[0, 1], [1, 0], [1, 1], [2, 1]],
  ],
  // S
  [
    [[0, 1], [0, 2], [1, 0], [1, 1]],
    [[0, 1], [1, 1], [1, 2], [2, 2]],
    [[1, 1], [1, 2], [2, 0], [2, 1]],
    [[0, 0], [1, 0], [1, 1], [2, 1]],
  ],
  // Z
  [
    [[0, 0], [0, 1], [1, 1], [1, 2]],
    [[0, 2], [1, 1], [1, 2], [2, 1]],
    [[1, 0], [1, 1], [2, 1], [2, 2]],
    [[0, 1], [1, 0], [1, 1], [2, 0]],
  ],
  // J
  [
    [[0, 0], [1, 0], [1, 1], [1, 2]],
    [[0, 1], [0, 2], [1, 1], [2, 1]],
    [[1, 0], [1, 1], [1, 2], [2, 2]],
    [[0, 1], [1, 1], [2, 0], [2, 1]],
  ],
  // L
  [
    [[0, 2], [1, 0], [1, 1], [1, 2]],
    [[0, 1], [1, 1], [2, 1], [2, 2]],
    [[1, 0], [1, 1], [1, 2], [2, 0]],
    [[0, 0], [0, 1], [1, 1], [2, 1]],
  ],
]

// SRS wall-kick tables. Offsets are [x, y] with x rightward, y UPWARD — a kick
// applies as `pr -= y; pc += x`. Indexed by the FROM-state of the rotation.
// JLSTZ share one table; I has its own; O never kicks. CW = state -> (state+1)
// mod 4; CCW = state -> (state+3) mod 4.
export const KICKS_JLSTZ_CW: number[][][] = [
  [[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]], // 0->1
  [[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]], // 1->2
  [[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]], // 2->3
  [[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]], // 3->0
]
export const KICKS_JLSTZ_CCW: number[][][] = [
  [[0, 0], [1, 0], [1, 1], [0, -2], [1, -2]], // 0->3
  [[0, 0], [1, 0], [1, -1], [0, 2], [1, 2]], // 1->0
  [[0, 0], [-1, 0], [-1, 1], [0, -2], [-1, -2]], // 2->1
  [[0, 0], [-1, 0], [-1, -1], [0, 2], [-1, 2]], // 3->2
]
export const KICKS_I_CW: number[][][] = [
  [[0, 0], [-2, 0], [1, 0], [-2, -1], [1, 2]], // 0->1
  [[0, 0], [-1, 0], [2, 0], [-1, 2], [2, -1]], // 1->2
  [[0, 0], [2, 0], [-1, 0], [2, 1], [-1, -2]], // 2->3
  [[0, 0], [1, 0], [-2, 0], [1, -2], [-2, 1]], // 3->0
]
export const KICKS_I_CCW: number[][][] = [
  [[0, 0], [-1, 0], [2, 0], [-1, 2], [2, -1]], // 0->3
  [[0, 0], [2, 0], [-1, 0], [2, 1], [-1, -2]], // 1->0
  [[0, 0], [1, 0], [-2, 0], [1, -2], [-2, 1]], // 2->1
  [[0, 0], [-2, 0], [1, 0], [-2, -1], [1, 2]], // 3->2
]

// Line-clear scoring, indexed by number of lines cleared (0..4), times level.
export const SCORES = [0, 100, 300, 500, 800]

// Hard-drop / soft-drop point rewards (per cell dropped), level-independent.
export const HARD_DROP_POINTS = 2
export const SOFT_DROP_POINTS = 1
