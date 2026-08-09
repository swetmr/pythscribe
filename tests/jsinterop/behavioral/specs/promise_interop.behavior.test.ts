// Track-A Promise-interop behavioral specs.
//
// Pins the END-TO-END authoring patterns an app model would actually write:
// Promise-based data loading in use_effect, asyncio.gather in an effect,
// Pythonic try/except around a rejecting fetch, and a .then/.catch/.finally
// chain (keyword attribute names, PR #179). Deterministic — "network" is
// setTimeout(0) / asyncio.sleep(<=0.02s).
//
// Run via: node tests/jsinterop/behavioral/run.mjs

import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen } from '@testing-library/react'
import { createElement } from 'react'

// vite-plugin-pyths compiles these on import.
import { DataLoader } from '../components/data_loader.ps'
import { GatherPanel } from '../components/gather_panel.ps'
import { ErrorState } from '../components/error_state.ps'
import { ChainLoader } from '../components/chain_loader.ps'

afterEach(cleanup)

describe('promise interop in components', () => {
  it('DataLoader: Promise resolved in use_effect renders the items', async () => {
    render(createElement(DataLoader as any))
    expect(screen.getByTestId('loading')).toBeTruthy()
    await screen.findByText('alpha')
    expect(screen.getByText('beta')).toBeTruthy()
    expect(screen.getByText('gamma')).toBeTruthy()
    expect(screen.queryByTestId('loading')).toBeNull()
  })

  it('GatherPanel: asyncio.gather in an effect, result in argument order', async () => {
    render(createElement(GatherPanel as any))
    expect(screen.getByTestId('loading')).toBeTruthy()
    // fetch_score (0.01s) finishes before fetch_user (0.02s); gather still
    // returns [user, score] in argument order.
    await screen.findByText('ada:99')
  })

  it('ErrorState: rejected promise surfaces an error state via try/except', async () => {
    render(createElement(ErrorState as any))
    expect(screen.getByTestId('loading')).toBeTruthy()
    await screen.findByText('failed: upstream down')
    expect(screen.queryByTestId('ready')).toBeNull()
  })

  it('ChainLoader: .then/.catch/.finally chain drives state', async () => {
    render(createElement(ChainLoader as any))
    expect(screen.getByTestId('loading')).toBeTruthy()
    await screen.findByText('HELLO')
  })
})
