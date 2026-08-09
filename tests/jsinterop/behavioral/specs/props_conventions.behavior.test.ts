// @component props-convention behavioral specs (#351 re-fix / #353 regression).
//
// Mounts the compiled components through REAL React (createElement with a
// flat props object — exactly how every call site lowers), so a definition/
// call-site convention mismatch fails HERE, not first in an application
// suite. This is the gate the #353 regression slipped through: reference-app's
// frontend (184 tests) caught arity-1 named-prop breakage that no
// compiler-side suite exercised.
//
// Run via: node tests/jsinterop/behavioral/run.mjs

import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen } from '@testing-library/react'
import { createElement } from 'react'

// vite-plugin-pyths compiles these on import.
import { Frontier, PaperLink, WholeProps, AliasProps } from '../components/props_conventions.ps'

afterEach(cleanup)

describe('@component props conventions', () => {
  it('arity-1 named param binds the PROP VALUE, not the props object (Frontier shape)', () => {
    render(createElement(Frontier as any, {
      data: { points: [{ x: 1, y: 2 }, { x: 3, y: 4 }] },
    }))
    const list = screen.getByTestId('frontier')
    expect(list.children.length).toBe(2)
    expect(screen.getByText('1,2')).toBeTruthy()
    expect(screen.getByText('3,4')).toBeTruthy()
  })

  it('arity-1 named prop interpolates its VALUE into an f-string URL (PaperUpload shape)', () => {
    render(createElement(PaperLink as any, { run_id: 'run-42' }))
    const link = screen.getByTestId('paper-link') as HTMLAnchorElement
    // Under #353's positional binding this was "/api/runs/[object Object]/paper.md".
    expect(link.getAttribute('href')).toBe('/api/runs/run-42/paper.md')
  })

  it('**props binds the whole flat props object (documented whole-object form)', () => {
    render(createElement(WholeProps as any, { label: 'runs', count: 7 }))
    expect(screen.getByTestId('whole').textContent).toBe('runs7')
  })

  it('a single no-default param named `props` is the whole-object alias', () => {
    render(createElement(AliasProps as any, { name: 'reference-app' }))
    expect(screen.getByTestId('alias').textContent).toBe('reference-app')
  })
})
