import { expect, type Page } from '@playwright/test'
import type { InteractionScript } from './types'

// Tetris — the timer/game-loop target the keyboard-only 2048 clone lacks.
// Gravity and lock delay are driven by setTimeout (GRAVITY_MS=1000,
// LOCK_MS=500), so Playwright's fake clock (page.clock, installed on BOTH
// pages) turns each gravity tick into a STABLE, byte-diffable checkpoint
// instead of a wall-clock race. advanceClock: 1000 fires exactly one gravity
// step (React defers the re-render + reschedule off the fake-timer loop), so
// the fall is single-stepped deterministically.
//
// The piece sequence is seeded (7-bag), so the same keys + same clock advances
// yield byte-identical DOM across the React oracle, .ps and .psc tracks; the
// lockstep runner asserts that per step. Expected cells were derived from the
// seed (SEED=1337): first piece T, next queue "LJSOZ".

async function press(page: Page, key: string): Promise<void> {
  await page.keyboard.press(key)
}

function color(p: Page, r: number, c: number) {
  return p.getByTestId('cell-' + r + '-' + c)
}

export const tetrisScript: InteractionScript = {
  clone: 'tetris',
  useClock: true,
  ready: async (page) => {
    await expect(page.getByTestId('tetris')).toBeVisible()
  },
  steps: [
    {
      name: 'new game (deterministic seed board: T on top)',
      run: (p) => p.getByTestId('restart').click(),
      settle: async (p) => {
        await expect(p.getByTestId('score-value')).toHaveText('0')
        await expect(p.getByTestId('next-queue')).toHaveText('LJSOZ')
        await expect(color(p, 0, 4)).toHaveAttribute('data-color', '3')
        await expect(color(p, 1, 3)).toHaveAttribute('data-color', '3')
      },
    },
    {
      name: 'gravity tick — T falls one row',
      run: async () => {},
      advanceClock: 1000,
      settle: async (p) => {
        await expect(color(p, 2, 3)).toHaveAttribute('data-color', '3')
        await expect(color(p, 1, 4)).toHaveAttribute('data-color', '3')
        await expect(color(p, 0, 4)).toHaveAttribute('data-filled', 'false')
      },
    },
    {
      name: 'gravity tick — T falls again',
      run: async () => {},
      advanceClock: 1000,
      settle: async (p) => {
        await expect(color(p, 3, 3)).toHaveAttribute('data-color', '3')
        await expect(color(p, 2, 4)).toHaveAttribute('data-color', '3')
      },
    },
    {
      name: 'ArrowLeft — shift T left',
      run: (p) => press(p, 'ArrowLeft'),
      settle: async (p) => {
        await expect(color(p, 3, 2)).toHaveAttribute('data-color', '3')
        await expect(color(p, 2, 3)).toHaveAttribute('data-color', '3')
      },
    },
    {
      name: 'ArrowUp — rotate T clockwise (SRS)',
      run: (p) => press(p, 'ArrowUp'),
      settle: async (p) => {
        await expect(color(p, 4, 3)).toHaveAttribute('data-color', '3')
        await expect(color(p, 3, 4)).toHaveAttribute('data-color', '3')
      },
    },
    {
      name: 'space — hard drop locks T, spawns L',
      run: (p) => press(p, 'Space'),
      settle: async (p) => {
        await expect(p.getByTestId('score-value')).toHaveText('30')
        await expect(p.getByTestId('next-queue')).toHaveText('JSOZI')
        await expect(color(p, 19, 3)).toHaveAttribute('data-color', '3') // locked T
        await expect(color(p, 18, 4)).toHaveAttribute('data-color', '3')
        await expect(color(p, 0, 5)).toHaveAttribute('data-color', '7') // new L
      },
    },
    {
      name: 'gravity tick — L falls one row',
      run: async () => {},
      advanceClock: 1000,
      settle: async (p) => {
        await expect(color(p, 2, 3)).toHaveAttribute('data-color', '7')
        await expect(color(p, 1, 5)).toHaveAttribute('data-color', '7')
      },
    },
    {
      name: 'gravity tick — L falls again',
      run: async () => {},
      advanceClock: 1000,
      settle: async (p) => {
        await expect(color(p, 3, 3)).toHaveAttribute('data-color', '7')
        await expect(color(p, 2, 5)).toHaveAttribute('data-color', '7')
      },
    },
    {
      name: 'ArrowRight — shift L right',
      run: (p) => press(p, 'ArrowRight'),
      settle: async (p) => {
        await expect(color(p, 3, 4)).toHaveAttribute('data-color', '7')
        await expect(color(p, 2, 6)).toHaveAttribute('data-color', '7')
      },
    },
    {
      name: 'space — hard drop locks L, spawns J',
      run: (p) => press(p, 'Space'),
      settle: async (p) => {
        await expect(p.getByTestId('score-value')).toHaveText('58')
        await expect(p.getByTestId('next-queue')).toHaveText('SOZIZ')
        await expect(color(p, 0, 3)).toHaveAttribute('data-color', '6') // new J
      },
    },
    {
      name: 'c — hold J, spawn S',
      run: (p) => press(p, 'c'),
      settle: async (p) => {
        await expect(p.getByTestId('hold-label')).toHaveText('J')
        await expect(p.getByTestId('next-queue')).toHaveText('OZIZJ')
        await expect(color(p, 0, 4)).toHaveAttribute('data-color', '4') // S
      },
    },
    {
      name: 'gravity tick — S falls one row',
      run: async () => {},
      advanceClock: 1000,
      settle: async (p) => {
        await expect(color(p, 2, 3)).toHaveAttribute('data-color', '4')
        await expect(color(p, 1, 4)).toHaveAttribute('data-color', '4')
      },
    },
  ],
}
