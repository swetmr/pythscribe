// Tri-track render-parity for SearchHeader (all components live in the
// single-module YouTubeApp.* tracks — see YouTubeApp.tsx header comment).

import { useState } from 'react'
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SearchHeader as ReactSearchHeader } from './YouTubeApp'
import { SearchHeader as PsSearchHeader } from './YouTubeApp.ps'
import { SearchHeader as PscSearchHeader } from './YouTubeApp.psc'

type Props = { query: string; on_change: (q: string) => void }

function contract(label: string, Component: React.ComponentType<Props>) {
  describe(label, () => {
    it('renders the logo and a search input bound to the query prop', () => {
      render(<Component query="lofi" on_change={() => {}} />)
      expect(screen.getByTestId('yt-header')).toHaveTextContent('MyTube')
      expect(screen.getByTestId('yt-search')).toHaveValue('lofi')
    })

    it('calls on_change with the typed value', async () => {
      const user = userEvent.setup()
      const on_change = vi.fn()
      render(<Component query="" on_change={on_change} />)
      await user.type(screen.getByTestId('yt-search'), 'r')
      expect(on_change).toHaveBeenCalledWith('r')
    })
  })
}

contract('SearchHeader.tsx (React reference)', ReactSearchHeader)
contract('SearchHeader.ps (PythScribe canonical)', PsSearchHeader)
contract('SearchHeader.psc (compressed PythScribe)', PscSearchHeader)

describe('SearchHeader dual-track DOM parity', () => {
  // SearchHeader is controlled — for the post-interaction parity check we
  // wrap it in a tiny stateful harness so typing changes rendered state.
  function Harness({ Component }: { Component: React.ComponentType<Props> }) {
    const [query, set_query] = useState('')
    return <Component query={query} on_change={set_query} />
  }

  async function snapshot(Component: React.ComponentType<Props>, type = false) {
    const user = userEvent.setup()
    const { container, unmount } = render(<Harness Component={Component} />)
    if (type) await user.type(screen.getByTestId('yt-search'), 'rust')
    const html = container.innerHTML
    const value = (screen.getByTestId('yt-search') as HTMLInputElement).value
    unmount()
    return { html, value }
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactSearchHeader)
    const p = await snapshot(PsSearchHeader)
    const c = await snapshot(PscSearchHeader)
    expect(p.html).toBe(r.html)
    expect(c.html).toBe(r.html)
  })

  it('post-typing DOM + input value match between React and PythScribe', async () => {
    const r = await snapshot(ReactSearchHeader, true)
    const p = await snapshot(PsSearchHeader, true)
    const c = await snapshot(PscSearchHeader, true)
    expect(p.html).toBe(r.html)
    expect(c.html).toBe(r.html)
    expect(r.value).toBe('rust')
    expect(p.value).toBe('rust')
    expect(c.value).toBe('rust')
  })
})
