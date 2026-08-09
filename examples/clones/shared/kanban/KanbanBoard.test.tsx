// Tri-track render-parity for the Kanban clone — same pattern as
// shared/hello/HelloCard.test.tsx (see CONTRIBUTING.md "per-clone contract").
// Mounts all 3 tracks and asserts equal behavior + equal DOM.
//
// Pointer-event DRAG is exercised in e2e/kanban.spec.ts (real browser —
// jsdom has no layout / no document.elementFromPoint). Here we cover the
// board CRUD surface: composer, inline edit, delete, rename, add column,
// count badges, reset, and localStorage persistence across remounts.

import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactKanbanBoard from './KanbanBoard'
import { KanbanBoard as PythKanbanBoard } from './KanbanBoard.ps'
import { KanbanBoard as PscKanbanBoard } from './KanbanBoard.psc'
import { KANBAN_STORAGE_KEY } from './fixtures'

type Track = React.ComponentType

beforeEach(() => {
  localStorage.clear()
})

function contract(label: string, Component: Track) {
  describe(label, () => {
    it('renders the fixture board: 3 columns, titles, cards, count badges', () => {
      render(<Component />)
      expect(screen.getByTestId('kb-col-title-col1')).toHaveTextContent('To Do')
      expect(screen.getByTestId('kb-col-title-col2')).toHaveTextContent('In Progress')
      expect(screen.getByTestId('kb-col-title-col3')).toHaveTextContent('Done')
      expect(screen.getByTestId('kb-count-col1')).toHaveTextContent('3')
      expect(screen.getByTestId('kb-count-col2')).toHaveTextContent('2')
      expect(screen.getByTestId('kb-count-col3')).toHaveTextContent('1')
      expect(screen.getByTestId('kb-card-c1')).toHaveTextContent('Design landing hero')
      expect(screen.getByTestId('kb-card-c6')).toHaveTextContent('Set up CI pipeline')
    })

    it('adds a card through the inline composer (Add button) and bumps the badge', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-add-card-col3'))
      await user.type(screen.getByTestId('kb-compose-input'), 'Ship the clone demo')
      await user.click(screen.getByTestId('kb-compose-add'))
      expect(screen.getByTestId('kb-card-c7')).toHaveTextContent('Ship the clone demo')
      expect(screen.getByTestId('kb-count-col3')).toHaveTextContent('2')
      // composer stays open for the next card (Trello behavior), input cleared
      expect(screen.getByTestId('kb-compose-input')).toHaveValue('')
    })

    it('adds a card with Enter and closes the composer with Escape', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-add-card-col1'))
      await user.type(screen.getByTestId('kb-compose-input'), 'Enter-added card{Enter}')
      expect(screen.getByTestId('kb-card-c7')).toHaveTextContent('Enter-added card')
      expect(screen.getByTestId('kb-count-col1')).toHaveTextContent('4')
      await user.keyboard('{Escape}')
      expect(screen.queryByTestId('kb-composer')).not.toBeInTheDocument()
    })

    it('edits a card inline: click opens textarea, Enter saves', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-card-c4'))
      const edit = screen.getByTestId('kb-edit-input')
      expect(edit).toHaveValue('Implement auth flow')
      await user.clear(edit)
      await user.type(edit, 'Implement OAuth flow{Enter}')
      expect(screen.queryByTestId('kb-edit-input')).not.toBeInTheDocument()
      expect(screen.getByTestId('kb-card-c4')).toHaveTextContent('Implement OAuth flow')
    })

    it('Escape cancels an inline edit without saving', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-card-c2'))
      const edit = screen.getByTestId('kb-edit-input')
      await user.clear(edit)
      await user.type(edit, 'should not persist')
      await user.keyboard('{Escape}')
      expect(screen.queryByTestId('kb-edit-input')).not.toBeInTheDocument()
      expect(screen.getByTestId('kb-card-c2')).toHaveTextContent('Write onboarding copy')
    })

    it('deletes a card and updates the badge', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-del-c5'))
      expect(screen.queryByTestId('kb-card-c5')).not.toBeInTheDocument()
      expect(screen.getByTestId('kb-count-col2')).toHaveTextContent('1')
    })

    it('renames a column via the inline input', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-col-title-col1'))
      const input = screen.getByTestId('kb-rename-input')
      expect(input).toHaveValue('To Do')
      await user.clear(input)
      await user.type(input, 'Backlog{Enter}')
      expect(screen.getByTestId('kb-col-title-col1')).toHaveTextContent('Backlog')
    })

    it('adds a new column with an empty card list', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-add-col'))
      await user.type(screen.getByTestId('kb-newcol-input'), 'Review{Enter}')
      expect(screen.getByTestId('kb-col-title-col4')).toHaveTextContent('Review')
      expect(screen.getByTestId('kb-count-col4')).toHaveTextContent('0')
    })

    it('persists to localStorage and restores across a remount', async () => {
      const user = userEvent.setup()
      const first = render(<Component />)
      await user.click(screen.getByTestId('kb-del-c1'))
      first.unmount()
      render(<Component />)
      expect(screen.queryByTestId('kb-card-c1')).not.toBeInTheDocument()
      expect(screen.getByTestId('kb-count-col1')).toHaveTextContent('2')
    })

    it('reset button restores the fixtures and clears saved state', async () => {
      const user = userEvent.setup()
      render(<Component />)
      await user.click(screen.getByTestId('kb-del-c6'))
      expect(screen.getByTestId('kb-count-col3')).toHaveTextContent('0')
      await user.click(screen.getByTestId('kb-reset'))
      expect(screen.getByTestId('kb-card-c6')).toHaveTextContent('Set up CI pipeline')
      expect(screen.getByTestId('kb-count-col3')).toHaveTextContent('1')
      expect(localStorage.getItem(KANBAN_STORAGE_KEY)).not.toBeNull() // reset writes fixtures through update_board
    })
  })
}

contract('KanbanBoard.tsx (React reference)', ReactKanbanBoard)
contract('KanbanBoard.ps (PythScribe canonical)', PythKanbanBoard)
contract('KanbanBoard.psc (compressed PythScribe)', PscKanbanBoard)

describe('Dual-track DOM parity', () => {
  async function snapshot(Component: Track, interact: 'none' | 'compose' | 'edit'): Promise<string> {
    localStorage.clear()
    const user = userEvent.setup()
    const { container, unmount } = render(<Component />)
    if (interact === 'compose') {
      await user.click(screen.getByTestId('kb-add-card-col2'))
      await user.type(screen.getByTestId('kb-compose-input'), 'Parity card{Enter}')
    } else if (interact === 'edit') {
      await user.click(screen.getByTestId('kb-card-c3'))
      await user.type(screen.getByTestId('kb-edit-input'), ' now')
    }
    const html = container.innerHTML
    unmount()
    localStorage.clear()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactKanbanBoard, 'none')
    const p = await snapshot(PythKanbanBoard, 'none')
    const c = await snapshot(PscKanbanBoard, 'none')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-add-card DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactKanbanBoard, 'compose')
    const p = await snapshot(PythKanbanBoard, 'compose')
    const c = await snapshot(PscKanbanBoard, 'compose')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('mid-inline-edit DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactKanbanBoard, 'edit')
    const p = await snapshot(PythKanbanBoard, 'edit')
    const c = await snapshot(PscKanbanBoard, 'edit')
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
