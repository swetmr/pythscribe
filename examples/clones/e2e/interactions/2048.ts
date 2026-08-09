import { expect, type Page } from '@playwright/test'
import type { InteractionScript } from './types'

// 2048 — a scripted keyboard walk over the seeded, deterministic board.
// Every tile spawn comes from a seeded PRNG (no Math.random / Date.now), so
// the same key sequence yields byte-identical DOM across all tracks; the
// lockstep runner asserts that per-step. No timers, so no useClock.
//
// Expected values below are the fixed constants for INITIAL_SEED (see
// Game2048.test.tsx for the derivation): seeds at (1,2) and (3,3), and the
// move-by-move score/board progression left→up→left→down→right.

async function press(page: Page, key: string): Promise<void> {
  await page.keyboard.press(key)
}

export const game2048Script: InteractionScript = {
  clone: '2048',
  ready: async (page) => {
    await expect(page.getByTestId('game-2048')).toBeVisible()
  },
  steps: [
    {
      name: 'new game (deterministic seed board)',
      run: (p) => p.getByTestId('restart').click(),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('0')
        await expect(p.getByTestId('score-value')).toHaveText('0')
        await expect(p.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
        await expect(p.getByTestId('cell-3-3')).toHaveAttribute('data-value', '2')
      },
    },
    {
      name: 'ArrowLeft — slide seed tiles to column 0',
      run: (p) => press(p, 'ArrowLeft'),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('1')
        await expect(p.getByTestId('cell-1-0')).toHaveAttribute('data-value', '2')
        await expect(p.getByTestId('cell-3-0')).toHaveAttribute('data-value', '2')
      },
    },
    {
      name: 'ArrowUp — merge the two 2s into a 4',
      run: (p) => press(p, 'ArrowUp'),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('2')
        await expect(p.getByTestId('score-value')).toHaveText('4')
        await expect(p.getByTestId('cell-0-0')).toHaveAttribute('data-value', '4')
      },
    },
    {
      name: 'ArrowLeft — compact the top row',
      run: (p) => press(p, 'ArrowLeft'),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('3')
        await expect(p.getByTestId('score-value')).toHaveText('4')
        await expect(p.getByTestId('cell-0-1')).toHaveAttribute('data-value', '2')
      },
    },
    {
      name: 'ArrowDown — merge column and score',
      run: (p) => press(p, 'ArrowDown'),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('4')
        await expect(p.getByTestId('score-value')).toHaveText('12')
      },
    },
    {
      name: 'ArrowRight — slide toward the right edge',
      run: (p) => press(p, 'ArrowRight'),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('5')
        await expect(p.getByTestId('score-value')).toHaveText('16')
      },
    },
    {
      name: 'reload — localStorage persistence',
      run: (p) => p.reload(),
      settle: async (p) => {
        await expect(p.getByTestId('game-2048')).toBeVisible()
        await expect(p.getByTestId('moves-value')).toHaveText('5')
        await expect(p.getByTestId('score-value')).toHaveText('16')
      },
    },
    {
      name: 'new game — reset back to the seed board',
      run: (p) => p.getByTestId('restart').click(),
      settle: async (p) => {
        await expect(p.getByTestId('moves-value')).toHaveText('0')
        await expect(p.getByTestId('score-value')).toHaveText('0')
        await expect(p.getByTestId('cell-1-2')).toHaveAttribute('data-value', '2')
      },
    },
  ],
}
