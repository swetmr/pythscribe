// Tri-track render-parity for the 2048 clone — same pattern as
// shared/kanban/KanbanBoard.test.tsx (see CONTRIBUTING.md "per-clone
// contract"). Mounts all 3 tracks and asserts equal behavior + byte-equal
// serialized DOM at initial render AND after representative key sequences.
//
// The whole board is derived from INITIAL_SEED via a seeded PRNG, so the
// expected tiles below are fixed constants — same on every track. Arrow
// keys are dispatched on window (jsdom bubbles keydown up), so no real
// browser is needed here; the raw-pointer path in Kanban has no analogue
// (2048 is keyboard-only).

import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactGame2048 from './Game2048'
import { Game2048 as PythGame2048 } from './Game2048.ps'
import { Game2048 as PscGame2048 } from './Game2048.psc'
import { GAME_STORAGE_KEY } from './fixtures'

type Track = React.ComponentType

beforeEach(() => {
  localStorage.clear()
})

function contract(label: string, Component: Track) {
  describe(label, () => {
    it('renders the deterministic fixture board: two seed tiles, zero score/moves', () => {
      render(<Component />)
      expect(screen.getByTestId('score-value')).toHaveTextContent('0')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('0')
      // seeded spawns land at (1,2) and (3,3)
      expect(screen.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('cell-0-0')).toHaveAttribute('data-value', '0')
      expect(screen.getByTestId('cell-0-0')).toHaveTextContent('')
      expect(screen.getByTestId('status')).toHaveTextContent('Use the arrow keys to combine tiles.')
      expect(screen.queryByTestId('overlay')).not.toBeInTheDocument()
    })

    it('ArrowLeft slides both seed tiles to column 0 and spawns a new tile', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.keyboard('{ArrowLeft}')
      expect(screen.getByTestId('cell-1-0')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('cell-3-0')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2') // deterministic spawn
      expect(screen.getByTestId('cell-1-2')).toHaveAttribute('data-value', '0')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('1')
      expect(screen.getByTestId('score-value')).toHaveTextContent('0')
    })

    it('ArrowLeft then ArrowUp merges two 2s into a 4 and scores 4', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.keyboard('{ArrowLeft}{ArrowUp}')
      expect(screen.getByTestId('cell-0-0')).toHaveAttribute('data-value', '4')
      expect(screen.getByTestId('cell-0-0')).toHaveTextContent('4')
      expect(screen.getByTestId('cell-0-3')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('score-value')).toHaveTextContent('4')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('2')
    })

    it('New Game resets the board, score and moves after some play', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.keyboard('{ArrowLeft}{ArrowUp}')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('2')
      await user.click(screen.getByTestId('restart'))
      expect(screen.getByTestId('score-value')).toHaveTextContent('0')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('0')
      expect(screen.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
      expect(screen.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
    })

    it('persists to localStorage and restores across a remount', async () => {
      const user = userEvent.setup()
      const first = render(<Component />)
      await user.keyboard('{ArrowLeft}{ArrowUp}')
      expect(screen.getByTestId('score-value')).toHaveTextContent('4')
      first.unmount()
      render(<Component />)
      expect(screen.getByTestId('score-value')).toHaveTextContent('4')
      expect(screen.getByTestId('moves-value')).toHaveTextContent('2')
      expect(localStorage.getItem(GAME_STORAGE_KEY)).not.toBeNull()
    })
  })
}

contract('Game2048.tsx (React reference)', ReactGame2048)
contract('Game2048.ps (PythScribe canonical)', PythGame2048)
contract('Game2048.psc (compressed PythScribe)', PscGame2048)

describe('Dual-track DOM parity', () => {
  async function snapshot(Component: Track, interact: 'none' | 'left' | 'leftup'): Promise<string> {
    localStorage.clear()
    const user = userEvent.setup()
    const { container, unmount } = render(<Component />)
    if (interact === 'left') {
      await user.keyboard('{ArrowLeft}')
    } else if (interact === 'leftup') {
      await user.keyboard('{ArrowLeft}{ArrowUp}')
    }
    const html = container.innerHTML
    unmount()
    localStorage.clear()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactGame2048, 'none')
    const p = await snapshot(PythGame2048, 'none')
    const c = await snapshot(PscGame2048, 'none')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-ArrowLeft DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactGame2048, 'left')
    const p = await snapshot(PythGame2048, 'left')
    const c = await snapshot(PscGame2048, 'left')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-merge (Left+Up) DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactGame2048, 'leftup')
    const p = await snapshot(PythGame2048, 'leftup')
    const c = await snapshot(PscGame2048, 'leftup')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
