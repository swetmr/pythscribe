'use client'
import { useEffect, useRef, useState } from 'react'
import './KanbanBoard.css'
import { KANBAN_FIXTURE, KANBAN_STORAGE_KEY, type KanbanCard, type KanbanColumn } from './fixtures'

/**
 * React reference oracle for the Kanban clone (Trello-like).
 * Dual-track-paired with KanbanBoard.ps / KanbanBoard.psc — all three must
 * render identical DOM for the same state (see KanbanBoard.test.tsx).
 *
 * Deliberate stress surface: card drag-and-drop is implemented with RAW
 * POINTER EVENTS (pointerdown / window pointermove / pointerup, Escape
 * cancels) — no dnd library. A floating ghost follows the pointer, the
 * hovered column gets a dropzone highlight, and drops reorder within a
 * column or move across columns. Board state persists to localStorage;
 * the Reset button restores the fixtures (e2e determinism hook).
 */

interface DragState {
  cardId: string
  fromCol: string
  text: string
  startX: number
  startY: number
  x: number
  y: number
  offsetX: number
  offsetY: number
  width: number
  active: boolean
  overCol: string | null
  overIndex: number
}

interface EditState {
  cardId: string
  text: string
}

function nextCardId(columns: KanbanColumn[]): string {
  let n = 0
  for (const col of columns) {
    for (const card of col.cards) {
      const v = Number(card.id.slice(1))
      if (v > n) n = v
    }
  }
  return 'c' + (n + 1)
}

function nextColId(columns: KanbanColumn[]): string {
  let n = 0
  for (const col of columns) {
    const v = Number(col.id.slice(3))
    if (v > n) n = v
  }
  return 'col' + (n + 1)
}

// Hit-test the pointer position against the rendered columns: returns the
// hovered column id plus the insertion index among that column's cards
// (the dragged card itself is excluded from the count, matching the
// remove-then-insert order moveCard applies on drop).
function hitTest(x: number, y: number, draggedId: string): [string | null, number] {
  const el = document.elementFromPoint(x, y)
  if (el === null) return [null, 0]
  const colEl = el.closest('[data-col-id]')
  if (colEl === null) return [null, 0]
  const colId = colEl.getAttribute('data-col-id')
  const cards = colEl.querySelectorAll('[data-card-id]')
  let idx = 0
  for (const cardEl of cards) {
    if (cardEl.getAttribute('data-card-id') === draggedId) continue
    const r = cardEl.getBoundingClientRect()
    if (y < r.top + r.height / 2) break
    idx += 1
  }
  return [colId, idx]
}

function moveCard(columns: KanbanColumn[], cardId: string, toCol: string, index: number): KanbanColumn[] {
  let card: KanbanCard | null = null
  for (const col of columns) {
    for (const c of col.cards) {
      if (c.id === cardId) card = c
    }
  }
  if (card === null) return columns
  const without = columns.map((col) => ({ ...col, cards: col.cards.filter((c) => c.id !== cardId) }))
  return without.map((col) =>
    col.id === toCol ? { ...col, cards: [...col.cards.slice(0, index), card, ...col.cards.slice(index)] } : col,
  )
}

export function KanbanBoard() {
  const [columns, setColumns] = useState<KanbanColumn[]>(() => structuredClone(KANBAN_FIXTURE))
  const [drag, setDrag] = useState<DragState | null>(null)
  const [editing, setEditing] = useState<EditState | null>(null)
  const [composing, setComposing] = useState<string | null>(null)
  const [composeText, setComposeText] = useState('')
  const [renaming, setRenaming] = useState<string | null>(null)
  const [renameText, setRenameText] = useState('')
  const [addingCol, setAddingCol] = useState(false)
  const [newColText, setNewColText] = useState('')
  const didDragRef = useRef(false)

  // localStorage persistence — load once after mount (client-only, so the
  // Next SSR pass and hydration both render the fixtures). Saving is
  // WRITE-THROUGH at each mutation site (updateBoard) rather than a
  // [columns]-effect: a save-effect's mount run closes over the initial
  // fixtures and clobbers the saved board before the load's re-render
  // lands (guaranteed under StrictMode's double-effect pass).
  useEffect(() => {
    try {
      const raw = localStorage.getItem(KANBAN_STORAGE_KEY)
      if (raw !== null) {
        const saved = JSON.parse(raw)
        if (Array.isArray(saved)) setColumns(saved)
      }
    } catch {
      // corrupt storage — keep fixtures
    }
  }, [])

  const updateBoard = (next: KanbanColumn[]) => {
    setColumns(next)
    localStorage.setItem(KANBAN_STORAGE_KEY, JSON.stringify(next))
  }

  // Pointer-event drag machine. Listeners live on window for the whole
  // drag so fast pointer moves can't escape the card element. Reattached
  // per state change (deps) — cheap, and keeps the handlers closure-fresh.
  useEffect(() => {
    if (drag === null) return () => null
    const onMove = (e: PointerEvent) => {
      if (!drag.active) {
        const dx = e.clientX - drag.startX
        const dy = e.clientY - drag.startY
        if (dx * dx + dy * dy < 25) return
        didDragRef.current = true
      }
      e.preventDefault()
      const [overCol, overIndex] = hitTest(e.clientX, e.clientY, drag.cardId)
      setDrag({ ...drag, active: true, x: e.clientX, y: e.clientY, overCol, overIndex })
    }
    const onUp = () => {
      if (drag.active && drag.overCol !== null) {
        updateBoard(moveCard(columns, drag.cardId, drag.overCol, drag.overIndex))
      }
      setDrag(null)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setDrag(null)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('keydown', onKey)
    }
  }, [drag, columns])

  const onCardPointerDown = (e: React.PointerEvent, card: KanbanCard, colId: string) => {
    if (e.button !== 0) return
    if ((e.target as Element).closest('button, textarea, input')) return
    const rect = e.currentTarget.getBoundingClientRect()
    didDragRef.current = false
    setDrag({
      cardId: card.id,
      fromCol: colId,
      text: card.text,
      startX: e.clientX,
      startY: e.clientY,
      x: e.clientX,
      y: e.clientY,
      offsetX: e.clientX - rect.left,
      offsetY: e.clientY - rect.top,
      width: rect.width,
      active: false,
      overCol: null,
      overIndex: 0,
    })
  }

  const onCardClick = (card: KanbanCard) => {
    if (didDragRef.current) {
      didDragRef.current = false
      return
    }
    setEditing({ cardId: card.id, text: card.text })
  }

  const saveEdit = () => {
    if (editing === null) return
    const text = editing.text.trim()
    if (text) {
      updateBoard(
        columns.map((col) => ({
          ...col,
          cards: col.cards.map((c) => (c.id === editing.cardId ? { ...c, text } : c)),
        })),
      )
    }
    setEditing(null)
  }

  const onEditKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      saveEdit()
    } else if (e.key === 'Escape') {
      setEditing(null)
    }
  }

  const deleteCard = (e: React.MouseEvent, cardId: string) => {
    e.stopPropagation()
    updateBoard(columns.map((col) => ({ ...col, cards: col.cards.filter((c) => c.id !== cardId) })))
  }

  const addCard = (colId: string) => {
    const text = composeText.trim()
    if (!text) return
    const cid = nextCardId(columns)
    updateBoard(columns.map((col) => (col.id === colId ? { ...col, cards: [...col.cards, { id: cid, text }] } : col)))
    setComposeText('')
  }

  const onComposeKey = (e: React.KeyboardEvent, colId: string) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      addCard(colId)
    } else if (e.key === 'Escape') {
      setComposing(null)
      setComposeText('')
    }
  }

  const startRename = (col: KanbanColumn) => {
    setRenaming(col.id)
    setRenameText(col.title)
  }

  const saveRename = () => {
    const text = renameText.trim()
    if (text) {
      updateBoard(columns.map((col) => (col.id === renaming ? { ...col, title: text } : col)))
    }
    setRenaming(null)
  }

  const onRenameKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      saveRename()
    } else if (e.key === 'Escape') {
      setRenaming(null)
    }
  }

  const addColumn = () => {
    const text = newColText.trim()
    if (!text) return
    updateBoard([...columns, { id: nextColId(columns), title: text, cards: [] }])
    setNewColText('')
    setAddingCol(false)
  }

  const onNewColKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      addColumn()
    } else if (e.key === 'Escape') {
      setAddingCol(false)
      setNewColText('')
    }
  }

  const resetBoard = () => {
    updateBoard(structuredClone(KANBAN_FIXTURE))
    setDrag(null)
    setEditing(null)
    setComposing(null)
    setComposeText('')
    setRenaming(null)
    setAddingCol(false)
    setNewColText('')
  }

  return (
    <div
      className={'kb-board' + (drag !== null && drag.active ? ' kb-board--dragging' : '')}
      data-testid="kanban-board"
    >
      <div className="kb-toolbar">
        <h1 className="kb-title">Kanban</h1>
        <button className="kb-reset" data-testid="kb-reset" onClick={resetBoard}>
          Reset board
        </button>
      </div>
      <div className="kb-cols" data-testid="kb-cols">
        {columns.map((col) => (
          <section
            key={col.id}
            className={
              'kb-col' + (drag !== null && drag.active && drag.overCol === col.id ? ' kb-col--drop' : '')
            }
            data-col-id={col.id}
            data-testid={'kb-col-' + col.id}
          >
            <header className="kb-col-head">
              {renaming === col.id ? (
                <input
                  className="kb-rename"
                  data-testid="kb-rename-input"
                  value={renameText}
                  autoFocus
                  onChange={(e) => setRenameText(e.target.value)}
                  onKeyDown={onRenameKey}
                />
              ) : (
                <h2 className="kb-col-title" data-testid={'kb-col-title-' + col.id} onClick={() => startRename(col)}>
                  {col.title}
                </h2>
              )}
              <span className="kb-count" data-testid={'kb-count-' + col.id}>
                {col.cards.length}
              </span>
            </header>
            <div className="kb-cards">
              {col.cards.map((card) =>
                editing !== null && editing.cardId === card.id ? (
                  <div key={card.id} className="kb-card kb-card--editing" data-card-id={card.id}>
                    <textarea
                      className="kb-edit"
                      data-testid="kb-edit-input"
                      value={editing.text}
                      autoFocus
                      onChange={(e) => setEditing({ cardId: card.id, text: e.target.value })}
                      onKeyDown={onEditKey}
                    />
                  </div>
                ) : (
                  <div
                    key={card.id}
                    className={
                      'kb-card' + (drag !== null && drag.active && drag.cardId === card.id ? ' kb-card--dragging' : '')
                    }
                    data-card-id={card.id}
                    data-testid={'kb-card-' + card.id}
                    onPointerDown={(e) => onCardPointerDown(e, card, col.id)}
                    onClick={() => onCardClick(card)}
                  >
                    <div className="kb-card-text">{card.text}</div>
                    <button className="kb-del" data-testid={'kb-del-' + card.id} onClick={(e) => deleteCard(e, card.id)}>
                      ×
                    </button>
                  </div>
                ),
              )}
            </div>
            <div className="kb-col-foot">
              {composing === col.id ? (
                <div className="kb-composer" data-testid="kb-composer">
                  <textarea
                    className="kb-compose"
                    data-testid="kb-compose-input"
                    placeholder="Enter a card title"
                    value={composeText}
                    autoFocus
                    onChange={(e) => setComposeText(e.target.value)}
                    onKeyDown={(e) => onComposeKey(e, col.id)}
                  />
                  <div className="kb-composer-actions">
                    <button className="kb-compose-add" data-testid="kb-compose-add" onClick={() => addCard(col.id)}>
                      Add card
                    </button>
                    <button
                      className="kb-compose-cancel"
                      data-testid="kb-compose-cancel"
                      onClick={() => {
                        setComposing(null)
                        setComposeText('')
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <button
                  className="kb-add-card"
                  data-testid={'kb-add-card-' + col.id}
                  onClick={() => {
                    setComposing(col.id)
                    setComposeText('')
                  }}
                >
                  + Add card
                </button>
              )}
            </div>
          </section>
        ))}
        <section className="kb-col kb-col--new">
          {addingCol ? (
            <div className="kb-newcol" data-testid="kb-newcol">
              <input
                className="kb-newcol-input"
                data-testid="kb-newcol-input"
                placeholder="Column title"
                value={newColText}
                autoFocus
                onChange={(e) => setNewColText(e.target.value)}
                onKeyDown={onNewColKey}
              />
              <div className="kb-composer-actions">
                <button className="kb-newcol-add" data-testid="kb-newcol-add" onClick={addColumn}>
                  Add column
                </button>
                <button
                  className="kb-newcol-cancel"
                  data-testid="kb-newcol-cancel"
                  onClick={() => {
                    setAddingCol(false)
                    setNewColText('')
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button className="kb-add-col" data-testid="kb-add-col" onClick={() => setAddingCol(true)}>
              + Add column
            </button>
          )}
        </section>
      </div>
      {drag !== null && drag.active && (
        <div
          className="kb-ghost"
          data-testid="kb-ghost"
          style={{ left: drag.x - drag.offsetX, top: drag.y - drag.offsetY, width: drag.width }}
        >
          {drag.text}
        </div>
      )}
    </div>
  )
}

export default KanbanBoard
