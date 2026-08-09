// Tri-track render-parity for the Tetris clone — same pattern as
// shared/2048/Game2048.test.tsx (see CONTRIBUTING.md "per-clone contract").
// Mounts all 3 tracks and asserts equal behavior + byte-equal serialized DOM
// at initial render AND after representative moves / rotations / a hard drop /
// a hold / a full line clear.
//
// The whole piece sequence is derived from SEED via a seeded 7-bag, so the
// expected pieces below are fixed constants — same on every track. Keys are
// dispatched SYNCHRONOUSLY via fireEvent on window (never `await`), so the
// gravity/lock setTimeout timers (GRAVITY_MS/LOCK_MS) never get a macrotask
// tick and cannot fire mid-test: the serialized DOM stays stable between
// interactions. The timer-driven loop itself is covered in a real browser by
// e2e/interactions/tetris.ts under Playwright's page.clock.

import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import ReactTetris from './Tetris'
import { Tetris as PythTetris } from './Tetris.ps'
import { Tetris as PscTetris } from './Tetris.psc'

type Track = React.ComponentType

function key(k: string): void {
  fireEvent.keyDown(window, { key: k })
}

function cell(r: number, c: number): HTMLElement {
  return screen.getByTestId('cell-' + r + '-' + c)
}

function contract(label: string, Component: Track) {
  describe(label, () => {
    it('renders the deterministic seed board: T piece on top, zeroed stats', () => {
      render(<Component />)
      expect(screen.getByTestId('score-value')).toHaveTextContent('0')
      expect(screen.getByTestId('lines-value')).toHaveTextContent('0')
      expect(screen.getByTestId('level-value')).toHaveTextContent('1')
      expect(screen.getByTestId('next-queue')).toHaveTextContent('LJSOZ')
      expect(screen.getByTestId('hold-label')).toHaveTextContent('-')
      // first piece is a T (color 3) at spawn: (0,4) + (1,3)(1,4)(1,5)
      expect(cell(0, 4)).toHaveAttribute('data-color', '3')
      expect(cell(1, 3)).toHaveAttribute('data-color', '3')
      expect(cell(1, 4)).toHaveAttribute('data-color', '3')
      expect(cell(1, 5)).toHaveAttribute('data-color', '3')
      expect(cell(0, 0)).toHaveAttribute('data-filled', 'false')
      expect(screen.queryByTestId('overlay')).not.toBeInTheDocument()
    })

    it('ArrowLeft shifts the active piece one column left', () => {
      render(<Component />)
      key('ArrowLeft')
      expect(cell(0, 3)).toHaveAttribute('data-color', '3')
      expect(cell(1, 2)).toHaveAttribute('data-color', '3')
      expect(cell(1, 4)).toHaveAttribute('data-color', '3')
      expect(cell(0, 4)).toHaveAttribute('data-filled', 'false')
    })

    it('ArrowUp rotates the T clockwise (SRS state 1)', () => {
      render(<Component />)
      key('ArrowLeft')
      key('ArrowUp')
      // T rot 1 at pc=2: (0,3)(1,3)(1,4)(2,3)
      expect(cell(0, 3)).toHaveAttribute('data-color', '3')
      expect(cell(1, 3)).toHaveAttribute('data-color', '3')
      expect(cell(1, 4)).toHaveAttribute('data-color', '3')
      expect(cell(2, 3)).toHaveAttribute('data-color', '3')
    })

    it('hard drop locks the piece, scores the drop, and spawns the next (L)', () => {
      render(<Component />)
      key('ArrowLeft')
      key('ArrowUp')
      key(' ')
      // T rot1 dropped to the floor (dist 17 -> 34 pts); occupies rows 17-19
      expect(screen.getByTestId('score-value')).toHaveTextContent('34')
      expect(cell(17, 3)).toHaveAttribute('data-color', '3')
      expect(cell(18, 3)).toHaveAttribute('data-color', '3')
      expect(cell(18, 4)).toHaveAttribute('data-color', '3')
      expect(cell(19, 3)).toHaveAttribute('data-color', '3')
      // next piece is an L (color 7), spawned at the top
      expect(cell(1, 3)).toHaveAttribute('data-color', '7')
      expect(screen.getByTestId('next-queue')).toHaveTextContent('JSOZI')
    })

    it('hold swaps the active piece into the hold slot', () => {
      render(<Component />)
      key('c')
      // T moves to hold; the next piece (L, color 7) becomes active
      expect(screen.getByTestId('hold-label')).toHaveTextContent('T')
      expect(cell(1, 3)).toHaveAttribute('data-color', '7')
      expect(screen.getByTestId('next-queue')).toHaveTextContent('JSOZI')
    })

    it('clears a completed line and scores it (single = 100 * level)', () => {
      render(<Component />)
      // Build the bottom row, then drop a vertical I into the last gap.
      key('ArrowLeft'); key('ArrowLeft'); key('ArrowLeft'); key(' ') // T far left
      key(' ') // L center
      key('ArrowRight'); key('ArrowRight'); key('ArrowRight'); key(' ') // J right
      key(' '); key(' '); key(' ') // S, O, Z pile up left of col 9
      key('ArrowUp') // rotate I to vertical
      for (let i = 0; i < 6; i++) key('ArrowRight') // slam I to the right wall (col 9)
      key(' ') // drop I -> completes + clears the bottom row
      expect(screen.getByTestId('lines-value')).toHaveTextContent('1')
      expect(screen.getByTestId('score-value')).toHaveTextContent('330')
    })

    it('New Game resets the board and stats after some play', () => {
      render(<Component />)
      key('ArrowLeft')
      key('ArrowUp')
      key(' ')
      expect(screen.getByTestId('score-value')).toHaveTextContent('34')
      fireEvent.click(screen.getByTestId('restart'))
      expect(screen.getByTestId('score-value')).toHaveTextContent('0')
      expect(screen.getByTestId('lines-value')).toHaveTextContent('0')
      expect(screen.getByTestId('next-queue')).toHaveTextContent('LJSOZ')
      expect(cell(0, 4)).toHaveAttribute('data-color', '3')
    })
  })
}

contract('Tetris.tsx (React reference)', ReactTetris)
contract('Tetris.ps (PythScribe canonical)', PythTetris)
contract('Tetris.psc (compressed PythScribe)', PscTetris)

describe('Tri-track DOM parity', () => {
  function snapshot(Component: Track, interact: 'none' | 'left' | 'lockspawn'): string {
    const { container, unmount } = render(<Component />)
    if (interact === 'left') {
      key('ArrowLeft')
    } else if (interact === 'lockspawn') {
      key('ArrowLeft')
      key('ArrowUp')
      key(' ')
    }
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', () => {
    const r = snapshot(ReactTetris, 'none')
    const p = snapshot(PythTetris, 'none')
    const c = snapshot(PscTetris, 'none')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-ArrowLeft DOM matches between React and PythScribe', () => {
    const r = snapshot(ReactTetris, 'left')
    const p = snapshot(PythTetris, 'left')
    const c = snapshot(PscTetris, 'left')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-lock+spawn DOM (hard drop) matches between React and PythScribe', () => {
    const r = snapshot(ReactTetris, 'lockspawn')
    const p = snapshot(PythTetris, 'lockspawn')
    const c = snapshot(PscTetris, 'lockspawn')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
