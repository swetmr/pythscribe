// Tri-track render-parity — the scaffold-proof pattern every clone
// component's test file follows (see CONTRIBUTING.md "Test template").
// Mounts all 3 tracks with the SAME props and asserts equal behavior +
// equal DOM.

import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactHelloCard from './HelloCard'
import { HelloCard as PythHelloCard } from './HelloCard.ps'
import { HelloCard as PscHelloCard } from './HelloCard.psc'
import { helloFixture } from './fixtures'

function contract(label: string, Component: React.ComponentType<{ title: string; subtitle?: string }>) {
  describe(label, () => {
    it('renders title + subtitle from props', () => {
      render(<Component {...helloFixture} />)
      expect(screen.getByTestId('hello-title')).toHaveTextContent(helloFixture.title)
      expect(screen.getByTestId('hello-subtitle')).toHaveTextContent(helloFixture.subtitle)
    })

    it('omits the subtitle node when subtitle is not passed', () => {
      render(<Component title="No subtitle" />)
      expect(screen.queryByTestId('hello-subtitle')).not.toBeInTheDocument()
    })

    it('toggles liked state on click', async () => {
      const user = userEvent.setup()
      render(<Component {...helloFixture} />)
      const btn = screen.getByTestId('hello-like')
      expect(btn).toHaveTextContent('♡ Like')
      await user.click(btn)
      expect(btn).toHaveTextContent('♥ Liked')
      await user.click(btn)
      expect(btn).toHaveTextContent('♡ Like')
    })
  })
}

contract('HelloCard.tsx (React reference)', ReactHelloCard)
contract('HelloCard.ps (PythScribe canonical)', PythHelloCard)
contract('HelloCard.psc (compressed PythScribe)', PscHelloCard)

describe('Dual-track DOM parity (scaffold regression guard)', () => {
  async function snapshot(Component: React.ComponentType<{ title: string; subtitle?: string }>, like = false): Promise<string> {
    const user = userEvent.setup()
    const { container, unmount } = render(<Component {...helloFixture} />)
    if (like) await user.click(screen.getByTestId('hello-like'))
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactHelloCard)
    const p = await snapshot(PythHelloCard)
    const c = await snapshot(PscHelloCard)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-click DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactHelloCard, true)
    const p = await snapshot(PythHelloCard, true)
    const c = await snapshot(PscHelloCard, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
