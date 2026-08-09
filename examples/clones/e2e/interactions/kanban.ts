import { expect, type Page } from '@playwright/test'
import type { InteractionScript } from './types'

// Kanban — CRUD + rename + new column + raw pointer-event drag + reload
// persistence. Card/column ids are sequential (c7, col4, ...), so the DOM
// stays byte-comparable across tracks after creation steps.

async function center(page: Page, testId: string): Promise<{ x: number; y: number; h: number }> {
  const box = await page.getByTestId(testId).boundingBox()
  if (!box) throw new Error('no bounding box for ' + testId)
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, h: box.height }
}

async function dragCard(page: Page, fromId: string, toId: string, topHalf: boolean): Promise<void> {
  const from = await center(page, fromId)
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move(from.x + 12, from.y + 12, { steps: 3 }) // past 5px activation
  await expect(page.getByTestId('kb-ghost')).toBeVisible()
  const to = await center(page, toId)
  const yOff = topHalf ? -to.h / 2 + 4 : to.h / 2 - 4
  await page.mouse.move(to.x, to.y + yOff, { steps: 10 })
  await page.mouse.up()
  await expect(page.getByTestId('kb-ghost')).toHaveCount(0)
}

export const kanbanScript: InteractionScript = {
  clone: 'kanban',
  ready: async (page) => {
    await expect(page.getByTestId('kanban-board')).toBeVisible()
  },
  steps: [
    {
      name: 'reset board to fixtures',
      run: (p) => p.getByTestId('kb-reset').click(),
      settle: async (p) => {
        await expect(p.getByTestId('kb-count-col1')).toHaveText('3')
      },
    },
    {
      name: 'open composer (col3)',
      run: (p) => p.getByTestId('kb-add-card-col3').click(),
      settle: async (p) => {
        await expect(p.getByTestId('kb-compose-input')).toBeVisible()
      },
    },
    {
      name: 'type card text into composer',
      run: (p) => p.getByTestId('kb-compose-input').fill('Parity card'),
      settle: async (p) => {
        await expect(p.getByTestId('kb-compose-input')).toHaveValue('Parity card')
      },
    },
    {
      name: 'submit card (Add button)',
      run: (p) => p.getByTestId('kb-compose-add').click(),
      settle: async (p) => {
        await expect(p.getByTestId('kb-card-c7')).toContainText('Parity card')
        await expect(p.getByTestId('kb-count-col3')).toHaveText('2')
      },
    },
    {
      name: 'open inline editor on c6',
      run: (p) => p.getByTestId('kb-card-c6').click(),
      settle: async (p) => {
        await expect(p.getByTestId('kb-edit-input')).toHaveValue('Set up CI pipeline')
      },
    },
    {
      name: 'edit card text and press Enter',
      run: async (p) => {
        await p.getByTestId('kb-edit-input').fill('Set up CD pipeline')
        await p.getByTestId('kb-edit-input').press('Enter')
      },
      settle: async (p) => {
        await expect(p.getByTestId('kb-card-c6')).toContainText('Set up CD pipeline')
      },
    },
    {
      name: 'delete card c5 (hover-revealed button)',
      run: async (p) => {
        await p.getByTestId('kb-card-c5').hover()
        await p.getByTestId('kb-del-c5').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('kb-count-col2')).toHaveText('1')
      },
    },
    {
      name: 'rename column col1 and press Enter',
      run: async (p) => {
        await p.getByTestId('kb-col-title-col1').click()
        await p.getByTestId('kb-rename-input').fill('Backlog')
        await p.getByTestId('kb-rename-input').press('Enter')
      },
      settle: async (p) => {
        await expect(p.getByTestId('kb-col-title-col1')).toHaveText('Backlog')
      },
    },
    {
      name: 'add a new column',
      run: async (p) => {
        await p.getByTestId('kb-add-col').click()
        await p.getByTestId('kb-newcol-input').fill('QA')
        await p.getByTestId('kb-newcol-add').click()
      },
      settle: async (p) => {
        await expect(p.getByTestId('kb-col-title-col4')).toHaveText('QA')
      },
    },
    {
      name: 'pointer-drag c1 into col2 (top of c4)',
      run: (p) => dragCard(p, 'kb-card-c1', 'kb-card-c4', true),
      settle: async (p) => {
        await expect(p.getByTestId('kb-count-col2')).toHaveText('2')
        await expect(p.getByTestId('kb-count-col1')).toHaveText('2')
      },
    },
    {
      name: 'Escape cancels an in-flight drag',
      run: async (p) => {
        const from = await center(p, 'kb-card-c2')
        await p.mouse.move(from.x, from.y)
        await p.mouse.down()
        await p.mouse.move(from.x + 12, from.y + 12, { steps: 3 })
        await expect(p.getByTestId('kb-ghost')).toBeVisible()
        await p.keyboard.press('Escape')
        await p.mouse.up()
      },
      settle: async (p) => {
        await expect(p.getByTestId('kb-ghost')).toHaveCount(0)
        await expect(p.getByTestId('kb-count-col1')).toHaveText('2')
      },
    },
    {
      name: 'reload — localStorage persistence',
      run: async (p) => {
        await p.reload()
      },
      settle: async (p) => {
        await expect(p.getByTestId('kanban-board')).toBeVisible()
        await expect(p.getByTestId('kb-count-col2')).toHaveText('2')
        await expect(p.getByTestId('kb-col-title-col1')).toHaveText('Backlog')
      },
    },
  ],
}
