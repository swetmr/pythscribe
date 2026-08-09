'use client'
import { useEffect, useState } from 'react'
import './Game2048.css'
import {
  GAME_STORAGE_KEY,
  INITIAL_SEED,
  WIN_TILE,
  type Dir,
  type GameState,
} from './fixtures'

/**
 * React reference oracle for the 2048 clone.
 * Dual-track-paired with Game2048.ps / Game2048.psc — all three must render
 * byte-identical DOM for the same key sequence (see Game2048.test.tsx).
 *
 * Deliberate stress surface for the compiler:
 *  - a SEEDED PRNG (Park–Miller MINSTD, pure integer arithmetic — no
 *    Math.random, no bitwise `>>>`, no BigInt) drives every tile spawn, so
 *    the game is fully deterministic across tracks and reproducible under
 *    Playwright;
 *  - a non-trivial reducer: slide + merge over rows/columns in four
 *    directions, score accumulation, win/lose detection;
 *  - window keydown event handling (Arrow keys), conditional overlay
 *    rendering, and localStorage write-through persistence.
 *
 * No wall-clock, no timers, no continuous loop — moves are discrete and
 * keyboard-driven, so the serialized DOM is stable between interactions.
 */

// --- seeded PRNG (Park–Miller MINSTD) — pure, returns the next state ---
function nextState(s: number): number {
  return (s * 16807) % 2147483647
}

// Spawn one tile (90% → 2, 10% → 4) into a random empty cell, advancing the
// PRNG state twice (cell pick, then value pick). Returns the new board and
// the advanced state. All selection is integer-modulo — no float rounding.
function spawn(board: number[], state: number): [number[], number] {
  const empties: number[] = []
  for (let i = 0; i < 16; i++) if (board[i] === 0) empties.push(i)
  if (empties.length === 0) return [board, state]
  let s = nextState(state)
  const idx = empties[s % empties.length]
  s = nextState(s)
  const value = s % 10 === 0 ? 4 : 2
  const nb = board.slice()
  nb[idx] = value
  return [nb, s]
}

function newGame(): GameState {
  let board: number[] = []
  for (let i = 0; i < 16; i++) board.push(0)
  let state = INITIAL_SEED
  ;[board, state] = spawn(board, state)
  ;[board, state] = spawn(board, state)
  return { board, score: 0, state, status: 'playing', moves: 0 }
}

// The 4 lines to compress for a direction, each as 4 flat board indices in
// the order they merge toward (front of the list = destination edge).
function lineIndices(dir: Dir): number[][] {
  const lines: number[][] = []
  if (dir === 'left' || dir === 'right') {
    for (let r = 0; r < 4; r++) {
      const idxs = [r * 4 + 0, r * 4 + 1, r * 4 + 2, r * 4 + 3]
      lines.push(dir === 'right' ? [idxs[3], idxs[2], idxs[1], idxs[0]] : idxs)
    }
  } else {
    for (let c = 0; c < 4; c++) {
      const idxs = [0 * 4 + c, 1 * 4 + c, 2 * 4 + c, 3 * 4 + c]
      lines.push(dir === 'down' ? [idxs[3], idxs[2], idxs[1], idxs[0]] : idxs)
    }
  }
  return lines
}

// Slide non-zero values to the front, merging equal adjacent pairs once each
// (classic 2048 rule), then pad to length 4. Returns [line, scoreGained].
function compressLine(line: number[]): [number[], number] {
  const nums = line.filter((v) => v !== 0)
  const res: number[] = []
  let gain = 0
  let i = 0
  while (i < nums.length) {
    if (i + 1 < nums.length && nums[i] === nums[i + 1]) {
      const merged = nums[i] * 2
      res.push(merged)
      gain += merged
      i += 2
    } else {
      res.push(nums[i])
      i += 1
    }
  }
  while (res.length < 4) res.push(0)
  return [res, gain]
}

// Deep board compare — MUST be explicit: `a !== b` on arrays is a reference
// compare in JS (always true), which would make every move "change" the
// board. The .ps/.psc twins use the same helper for identical semantics.
function boardsEqual(a: number[], b: number[]): boolean {
  for (let i = 0; i < 16; i++) if (a[i] !== b[i]) return false
  return true
}

function hasMoves(board: number[]): boolean {
  for (let i = 0; i < 16; i++) if (board[i] === 0) return true
  for (let r = 0; r < 4; r++) {
    for (let c = 0; c < 4; c++) {
      const v = board[r * 4 + c]
      if (c < 3 && board[r * 4 + c + 1] === v) return true
      if (r < 3 && board[(r + 1) * 4 + c] === v) return true
    }
  }
  return false
}

// The reducer. Returns the SAME object reference when the move is a no-op
// (blocked line or non-playing status) so React bails out of re-render.
function step(game: GameState, dir: Dir): GameState {
  if (game.status !== 'playing') return game
  const lines = lineIndices(dir)
  const nb = game.board.slice()
  let gain = 0
  for (const idxs of lines) {
    const line = [game.board[idxs[0]], game.board[idxs[1]], game.board[idxs[2]], game.board[idxs[3]]]
    const [comp, g] = compressLine(line)
    gain += g
    for (let k = 0; k < 4; k++) nb[idxs[k]] = comp[k]
  }
  if (boardsEqual(nb, game.board)) return game
  const [board2, state2] = spawn(nb, game.state)
  const score = game.score + gain
  let won = false
  for (let i = 0; i < 16; i++) if (board2[i] === WIN_TILE) won = true
  let status: GameState['status'] = 'playing'
  if (won) status = 'won'
  else if (!hasMoves(board2)) status = 'lost'
  return { board: board2, score, state: state2, status, moves: game.moves + 1 }
}

function keyDir(key: string): Dir | null {
  if (key === 'ArrowLeft') return 'left'
  if (key === 'ArrowRight') return 'right'
  if (key === 'ArrowUp') return 'up'
  if (key === 'ArrowDown') return 'down'
  return null
}

export function Game2048() {
  const [game, setGame] = useState<GameState>(() => newGame())

  // localStorage load once after mount (client-only, so SSR + hydration
  // both render the fresh fixture game). Write-through on every mutation
  // via updateGame — never a [game]-effect (its mount run would clobber the
  // saved game with the fixtures before the load lands, exactly as the
  // Kanban clone documents for StrictMode's double-effect pass).
  useEffect(() => {
    try {
      const raw = localStorage.getItem(GAME_STORAGE_KEY)
      if (raw !== null) {
        const saved = JSON.parse(raw)
        if (saved && Array.isArray(saved.board) && saved.board.length === 16) {
          setGame(saved)
        }
      }
    } catch {
      // corrupt storage — keep the fixture game
    }
  }, [])

  const updateGame = (next: GameState) => {
    setGame(next)
    localStorage.setItem(GAME_STORAGE_KEY, JSON.stringify(next))
  }

  // Window keydown — re-attached per game state so the handler closes over
  // fresh state (same pattern as the Kanban drag machine). Arrow keys move;
  // other keys pass through (no preventDefault) so the page stays usable.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const dir = keyDir(e.key)
      if (dir === null) return
      e.preventDefault()
      updateGame(step(game, dir))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [game])

  const restart = () => {
    updateGame(newGame())
  }

  return (
    <div className="g2048" data-testid="game-2048">
      <div className="g2048-toolbar">
        <h1 className="g2048-title">2048</h1>
        <div className="g2048-stats">
          <div className="g2048-stat" data-testid="score">
            <span className="g2048-stat-label">Score</span>
            <span className="g2048-stat-value" data-testid="score-value">
              {game.score}
            </span>
          </div>
          <div className="g2048-stat" data-testid="moves">
            <span className="g2048-stat-label">Moves</span>
            <span className="g2048-stat-value" data-testid="moves-value">
              {game.moves}
            </span>
          </div>
        </div>
        <button className="g2048-restart" data-testid="restart" onClick={restart}>
          New Game
        </button>
      </div>
      <p className="g2048-hint" data-testid="status">
        {game.status === 'won'
          ? 'You reached 2048!'
          : game.status === 'lost'
            ? 'No moves left.'
            : 'Use the arrow keys to combine tiles.'}
      </p>
      <div className="g2048-grid" data-testid="grid">
        {game.board.map((v, i) => (
          <div
            key={i}
            className={'g2048-cell g2048-cell--' + v}
            data-testid={'cell-' + Math.floor(i / 4) + '-' + (i % 4)}
            data-value={v}
          >
            {v > 0 ? v : ''}
          </div>
        ))}
      </div>
      {game.status !== 'playing' && (
        <div className="g2048-overlay" data-testid="overlay">
          <div className="g2048-overlay-msg" data-testid="overlay-msg">
            {game.status === 'won' ? 'You win!' : 'Game over'}
          </div>
          <button className="g2048-overlay-btn" data-testid="overlay-restart" onClick={restart}>
            Try again
          </button>
        </div>
      )}
    </div>
  )
}

export default Game2048
