// Local constants ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). 2048 is a pure deterministic game: the whole board
// is derived from INITIAL_SEED via a seeded PRNG (Park–Miller / MINSTD), so
// every track produces byte-identical DOM for the same key sequence. No
// Math.random, no Date.now — the "fixture" here is the seed + the storage key.

export type Dir = 'left' | 'right' | 'up' | 'down'
export type GameStatus = 'playing' | 'won' | 'lost'

export interface GameState {
  board: number[] // flat 4x4, index = row*4 + col; 0 = empty
  score: number
  state: number // current PRNG state (seeded, deterministic)
  status: GameStatus
  moves: number
}

export const GAME_STORAGE_KEY = 'pyths-clones-2048'

// Park–Miller MINSTD seed. Chosen so the first two spawns land in stable,
// visually separated cells (see Game2048.test.tsx for the exact expected
// initial board). Kept < 2^31 so `state * 16807` stays within 2^53 — the
// arithmetic is bit-exact in both JS and PythScribe (no bitwise ops, no
// `>>>`, no BigInt divergence).
export const INITIAL_SEED = 12345

// Winning tile value. Reaching it flips status to 'won'.
export const WIN_TILE = 2048
