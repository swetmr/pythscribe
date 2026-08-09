'use client'
import { useEffect, useState } from 'react'
import './Tetris.css'
import {
  COLS,
  GRAVITY_MS,
  HARD_DROP_POINTS,
  KICKS_I_CCW,
  KICKS_I_CW,
  KICKS_JLSTZ_CCW,
  KICKS_JLSTZ_CW,
  LETTERS,
  LOCK_MS,
  PIECES,
  PREVIEW,
  ROWS,
  SCORES,
  SEED,
  SOFT_DROP_POINTS,
  SPAWN_COL,
  SPAWN_ROW,
  type GameState,
} from './fixtures'

/**
 * React reference oracle for the Tetris clone.
 * Dual-track-paired with Tetris.ps / Tetris.psc — all three must render
 * byte-identical DOM for the same key sequence + clock advances (see
 * Tetris.test.tsx and e2e/interactions/tetris.ts).
 *
 * Deliberate stress surface for the compiler — everything 2048 has PLUS a real
 * timer-driven game loop:
 *  - a SEEDED 7-bag (Park–Miller MINSTD + Fisher–Yates, pure integer
 *    arithmetic — no Math.random, no bitwise ops, no BigInt) so the piece
 *    sequence is fully deterministic and reproducible under Playwright;
 *  - SRS rotation with the standard wall-kick tables (data in fixtures.ts);
 *  - a gravity + lock-delay reducer driven by setTimeout, controllable by a
 *    fake clock (page.clock in e2e) — this is the async/timer lowering path
 *    that the keyboard-only 2048 clone does not exercise;
 *  - deterministic line-clear, scoring and leveling; hold piece; next queue.
 *
 * The gravity/lock timers are set large (GRAVITY_MS/LOCK_MS) so they never
 * fire during the fully-synchronous fireEvent-driven render-parity tests; the
 * e2e interaction differential advances the fake clock to fire them on
 * purpose. Same seed + same clock advances ⇒ byte-identical DOM on every track.
 */

// --- seeded PRNG (Park–Miller MINSTD) — pure, returns the next state ---
function prng(s: number): number {
  return (s * 16807) % 2147483647
}

// One shuffled bag of the 7 piece types (Fisher–Yates driven by the PRNG,
// pure integer-modulo). Returns [nextState, bag].
function newBag(seed: number): [number, number[]] {
  const bag = [0, 1, 2, 3, 4, 5, 6]
  let s = seed
  for (let i = 6; i > 0; i--) {
    s = prng(s)
    const j = s % (i + 1)
    const tmp = bag[i]
    bag[i] = bag[j]
    bag[j] = tmp
  }
  return [s, bag]
}

// Refill the upcoming-piece queue until it holds at least a full bag, so the
// PREVIEW window and the next spawn are always available. Returns [seed, queue].
function ensureQueue(seed: number, queue: number[]): [number, number[]] {
  let s = seed
  let q = queue.slice()
  while (q.length < 7) {
    const [s2, bag] = newBag(s)
    s = s2
    q = q.concat(bag)
  }
  return [s, q]
}

// Board cells [row, col] occupied by a piece of `type` at rotation `rot` whose
// local box top-left sits at [pr, pc].
function pieceCells(type: number, rot: number, pr: number, pc: number): number[][] {
  return PIECES[type][rot].map((m) => [pr + m[0], pc + m[1]])
}

// A placement collides if any mino leaves the well sideways/below or overlaps a
// settled block. Rows above the top (r < 0) are open space (spawn/kick room).
function collides(board: number[], cells: number[][]): boolean {
  for (const cell of cells) {
    const r = cell[0]
    const c = cell[1]
    if (c < 0 || c >= COLS || r >= ROWS) return true
    if (r >= 0 && board[r * COLS + c] !== 0) return true
  }
  return false
}

// Remove full rows, dropping everything above down. Returns [board, cleared].
function clearLines(board: number[]): [number[], number] {
  const kept: number[][] = []
  let cleared = 0
  for (let r = 0; r < ROWS; r++) {
    const row = board.slice(r * COLS, r * COLS + COLS)
    let full = true
    for (let c = 0; c < COLS; c++) if (row[c] === 0) full = false
    if (full) cleared += 1
    else kept.push(row)
  }
  const out: number[] = []
  for (let i = 0; i < cleared; i++) for (let c = 0; c < COLS; c++) out.push(0)
  for (const row of kept) for (const v of row) out.push(v)
  return [out, cleared]
}

function newGame(): GameState {
  const board: number[] = []
  for (let i = 0; i < ROWS * COLS; i++) board.push(0)
  let [seed, queue] = ensureQueue(SEED, [])
  const current = queue[0]
  queue = queue.slice(1)
  ;[seed, queue] = ensureQueue(seed, queue)
  return {
    board,
    seed,
    queue,
    current,
    rot: 0,
    pr: SPAWN_ROW,
    pc: SPAWN_COL,
    hold: null,
    canHold: true,
    score: 0,
    lines: 0,
    level: 1,
    status: 'playing',
  }
}

// Lock the active piece into the board, clear lines, score, and spawn the next
// piece from the queue. Sets status 'over' if the fresh piece has no room.
function lockPiece(g: GameState): GameState {
  const board = g.board.slice()
  for (const cell of pieceCells(g.current, g.rot, g.pr, g.pc)) {
    board[cell[0] * COLS + cell[1]] = g.current + 1
  }
  const [board2, cleared] = clearLines(board)
  const lines = g.lines + cleared
  const level = Math.floor(lines / 10) + 1
  const score = g.score + SCORES[cleared] * g.level
  let [seed, queue] = ensureQueue(g.seed, g.queue)
  const next = queue[0]
  queue = queue.slice(1)
  ;[seed, queue] = ensureQueue(seed, queue)
  const ng: GameState = {
    board: board2,
    seed,
    queue,
    current: next,
    rot: 0,
    pr: SPAWN_ROW,
    pc: SPAWN_COL,
    hold: g.hold,
    canHold: true,
    score,
    lines,
    level,
    status: 'playing',
  }
  if (collides(board2, pieceCells(next, 0, SPAWN_ROW, SPAWN_COL))) ng.status = 'over'
  return ng
}

// Gravity/lock reducer — the setTimeout callback. Drop one row if possible,
// otherwise lock. Returns the same reference when not playing (no re-render).
function tick(g: GameState): GameState {
  if (g.status !== 'playing') return g
  if (!collides(g.board, pieceCells(g.current, g.rot, g.pr + 1, g.pc))) {
    return { ...g, pr: g.pr + 1 }
  }
  return lockPiece(g)
}

function move(g: GameState, dc: number): GameState {
  if (g.status !== 'playing') return g
  if (collides(g.board, pieceCells(g.current, g.rot, g.pr, g.pc + dc))) return g
  return { ...g, pc: g.pc + dc }
}

function softDrop(g: GameState): GameState {
  if (g.status !== 'playing') return g
  if (collides(g.board, pieceCells(g.current, g.rot, g.pr + 1, g.pc))) return g
  return { ...g, pr: g.pr + 1, score: g.score + SOFT_DROP_POINTS }
}

function hardDrop(g: GameState): GameState {
  if (g.status !== 'playing') return g
  let dist = 0
  while (!collides(g.board, pieceCells(g.current, g.rot, g.pr + dist + 1, g.pc))) dist += 1
  const dropped: GameState = { ...g, pr: g.pr + dist, score: g.score + dist * HARD_DROP_POINTS }
  return lockPiece(dropped)
}

function rotate(g: GameState, dir: number): GameState {
  if (g.status !== 'playing') return g
  if (g.current === 1) return g // O — rotation is a no-op
  const newRot = (g.rot + dir + 4) % 4
  const isI = g.current === 0
  let table: number[][][]
  if (dir === 1) table = isI ? KICKS_I_CW : KICKS_JLSTZ_CW
  else table = isI ? KICKS_I_CCW : KICKS_JLSTZ_CCW
  const kicks = table[g.rot]
  for (const k of kicks) {
    const npr = g.pr - k[1]
    const npc = g.pc + k[0]
    if (!collides(g.board, pieceCells(g.current, newRot, npr, npc))) {
      return { ...g, rot: newRot, pr: npr, pc: npc }
    }
  }
  return g
}

function holdPiece(g: GameState): GameState {
  if (g.status !== 'playing' || !g.canHold) return g
  let seed = g.seed
  let queue = g.queue
  let current: number
  const hold = g.current
  if (g.hold === null) {
    ;[seed, queue] = ensureQueue(seed, queue)
    current = queue[0]
    queue = queue.slice(1)
    ;[seed, queue] = ensureQueue(seed, queue)
  } else {
    current = g.hold
  }
  const ng: GameState = {
    ...g,
    hold,
    current,
    rot: 0,
    pr: SPAWN_ROW,
    pc: SPAWN_COL,
    canHold: false,
    seed,
    queue,
  }
  if (collides(g.board, pieceCells(current, 0, SPAWN_ROW, SPAWN_COL))) ng.status = 'over'
  return ng
}

// Discrete keyboard reducer — pure, returns the same reference on a no-op.
function reduceKey(g: GameState, key: string): GameState {
  if (key === 'ArrowLeft') return move(g, -1)
  if (key === 'ArrowRight') return move(g, 1)
  if (key === 'ArrowDown') return softDrop(g)
  if (key === 'ArrowUp' || key === 'x') return rotate(g, 1)
  if (key === 'z') return rotate(g, -1)
  if (key === ' ') return hardDrop(g)
  if (key === 'c') return holdPiece(g)
  return g
}

const HANDLED: Record<string, boolean> = {
  ArrowLeft: true,
  ArrowRight: true,
  ArrowUp: true,
  ArrowDown: true,
  ' ': true,
  x: true,
  z: true,
  c: true,
}

// Map of board index -> color for the active piece (cells above the top row
// are not rendered). Used to overlay the falling piece on the settled board.
function activeMap(g: GameState): Record<number, number> {
  const m: Record<number, number> = {}
  for (const cell of pieceCells(g.current, g.rot, g.pr, g.pc)) {
    if (cell[0] >= 0) m[cell[0] * COLS + cell[1]] = g.current + 1
  }
  return m
}

// 4x4 mini-grid data for a piece preview (spawn rotation), keyed by r*4+c.
function miniMap(type: number | null): Record<number, number> {
  const m: Record<number, number> = {}
  if (type === null) return m
  for (const cell of PIECES[type][0]) m[cell[0] * 4 + cell[1]] = type + 1
  return m
}

function Mini({ type, kind }: { type: number | null; kind: string }) {
  const m = miniMap(type)
  const cells = []
  for (let i = 0; i < 16; i++) {
    const color = m[i] ?? 0
    cells.push(
      <div
        key={i}
        className={'tet-mini-cell tet-color-' + color}
        data-testid={kind + '-cell-' + Math.floor(i / 4) + '-' + (i % 4)}
        data-color={color}
      />,
    )
  }
  return <div className="tet-mini">{cells}</div>
}

export function Tetris() {
  const [game, setGame] = useState<GameState>(() => newGame())

  // Discrete keyboard control — subscribed once; every handler is a functional
  // update over fresh state, so no [game] re-subscription is needed. Arrow
  // keys + space + x/z/c are consumed (preventDefault); everything else passes
  // through so the page stays usable.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!HANDLED[e.key]) return
      e.preventDefault()
      setGame((g) => reduceKey(g, e.key))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // The timer-driven game loop. Reschedules on every state change: LOCK_MS when
  // the piece is grounded (lock delay), GRAVITY_MS while it is still falling.
  // Under Playwright's page.clock these fire deterministically; under vitest
  // (synchronous fireEvent, no macrotask yield) they never fire mid-test.
  useEffect(() => {
    if (game.status !== 'playing') return undefined
    const grounded = collides(game.board, pieceCells(game.current, game.rot, game.pr + 1, game.pc))
    const delay = grounded ? LOCK_MS : GRAVITY_MS
    const id = setTimeout(() => setGame((g) => tick(g)), delay)
    return () => clearTimeout(id)
  }, [game])

  const restart = () => setGame(newGame())

  const amap = activeMap(game)
  const queueLetters = []
  for (let i = 0; i < PREVIEW; i++) queueLetters.push(LETTERS[game.queue[i]])

  const cells = []
  for (let i = 0; i < ROWS * COLS; i++) {
    const color = amap[i] ?? game.board[i]
    const filled = color !== 0
    cells.push(
      <div
        key={i}
        className={'tet-cell tet-color-' + color}
        data-testid={'cell-' + Math.floor(i / COLS) + '-' + (i % COLS)}
        data-filled={filled}
        data-color={color}
      />,
    )
  }

  return (
    <div className="tetris" data-testid="tetris">
      <div className="tet-side tet-side-left">
        <div className="tet-panel">
          <span className="tet-panel-label">Hold</span>
          <Mini type={game.hold} kind="hold" />
          <span className="tet-panel-sub" data-testid="hold-label">
            {game.hold === null ? '-' : LETTERS[game.hold]}
          </span>
        </div>
      </div>
      <div className="tet-center">
        <div className="tet-toolbar">
          <h1 className="tet-title">Tetris</h1>
          <button className="tet-restart" data-testid="restart" onClick={restart}>
            New Game
          </button>
        </div>
        <p className="tet-status" data-testid="status">
          {game.status === 'over' ? 'Game over.' : 'Arrows move, ↑/x rotate, z rotate CCW, space drop, c hold.'}
        </p>
        <div className="tet-board" data-testid="board">
          {cells}
        </div>
        {game.status === 'over' && (
          <div className="tet-overlay" data-testid="overlay">
            <div className="tet-overlay-msg" data-testid="overlay-msg">
              Game over
            </div>
            <button className="tet-overlay-btn" data-testid="overlay-restart" onClick={restart}>
              Try again
            </button>
          </div>
        )}
      </div>
      <div className="tet-side tet-side-right">
        <div className="tet-stats">
          <div className="tet-stat" data-testid="score">
            <span className="tet-panel-label">Score</span>
            <span className="tet-stat-value" data-testid="score-value">
              {game.score}
            </span>
          </div>
          <div className="tet-stat" data-testid="lines">
            <span className="tet-panel-label">Lines</span>
            <span className="tet-stat-value" data-testid="lines-value">
              {game.lines}
            </span>
          </div>
          <div className="tet-stat" data-testid="level">
            <span className="tet-panel-label">Level</span>
            <span className="tet-stat-value" data-testid="level-value">
              {game.level}
            </span>
          </div>
        </div>
        <div className="tet-panel">
          <span className="tet-panel-label">Next</span>
          <Mini type={game.queue[0]} kind="next" />
          <span className="tet-panel-sub" data-testid="next-queue">
            {queueLetters.join('')}
          </span>
        </div>
      </div>
    </div>
  )
}

export default Tetris
