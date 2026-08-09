// Tri-track render-parity for the portal detail modal — the A1-exercise
// component: `from react_dom import create_portal` in the .ps/.psc tracks.
// Asserts the portal really lands under document.body, Escape-to-close via
// the use_effect keydown listener, body-scroll lock + restore on cleanup,
// and the in-modal My List toggle (context read through the portal).
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { DetailModal as ReactModal, MyListProvider as ReactProvider } from './NetflixApp'
import { DetailModal as PsModal, MyListProvider as PsProvider } from './NetflixApp.ps'
import { DetailModal as PscModal, MyListProvider as PscProvider } from './NetflixApp.psc'
import { netflixTitles } from './fixtures'

const title = netflixTitles['s03']

type AnyComp = React.ComponentType<any>

function mount(Provider: AnyComp, Modal: AnyComp, on_close: () => void = () => {}) {
  return render(
    <Provider>
      <Modal title={title} on_close={on_close} />
    </Provider>,
  )
}

function contract(label: string, Provider: AnyComp, Modal: AnyComp) {
  describe(label, () => {
    it('renders title/desc/genres through a portal parented on document.body', () => {
      mount(Provider, Modal)
      expect(screen.getByTestId('nf-modal-title')).toHaveTextContent(title.name)
      expect(screen.getByTestId('nf-modal-desc')).toHaveTextContent(title.description)
      expect(screen.getByTestId('nf-modal')).toHaveTextContent(title.genres.join(' | '))
      const backdrop = screen.getByTestId('nf-modal-backdrop')
      expect(backdrop.parentElement).toBe(document.body)
      expect(screen.getByTestId('nf-modal')).toHaveAttribute('aria-modal', 'true')
    })

    it('locks body scroll while open and restores it on unmount', () => {
      const { unmount } = mount(Provider, Modal)
      expect(document.body.style.overflow).toBe('hidden')
      unmount()
      expect(document.body.style.overflow).toBe('')
    })

    it('calls on_close on Escape (document-level keydown listener)', () => {
      const on_close = vi.fn()
      mount(Provider, Modal, on_close)
      fireEvent.keyDown(document.body, { key: 'a' })
      expect(on_close).not.toHaveBeenCalled()
      fireEvent.keyDown(document.body, { key: 'Escape' })
      expect(on_close).toHaveBeenCalledTimes(1)
    })

    it('calls on_close on backdrop click but not on panel click (stopPropagation)', async () => {
      const user = userEvent.setup()
      const on_close = vi.fn()
      mount(Provider, Modal, on_close)
      await user.click(screen.getByTestId('nf-modal-title'))
      expect(on_close).not.toHaveBeenCalled()
      await user.click(screen.getByTestId('nf-modal-backdrop'))
      expect(on_close).toHaveBeenCalledTimes(1)
    })

    it('the in-modal toggle flips My List membership via context', async () => {
      const user = userEvent.setup()
      mount(Provider, Modal)
      const toggle = screen.getByTestId('nf-modal-toggle')
      expect(toggle).toHaveTextContent('+ Add to My List')
      await user.click(toggle)
      expect(toggle).toHaveTextContent('✓ In My List')
      await user.click(toggle)
      expect(toggle).toHaveTextContent('+ Add to My List')
    })
  })
}

contract('DetailModal.tsx (React reference)', ReactProvider, ReactModal)
contract('DetailModal .ps (PythScribe canonical)', PsProvider, PsModal)
contract('DetailModal .psc (compressed PythScribe)', PscProvider, PscModal)

describe('DetailModal dual-track DOM parity (portal HTML under document.body)', () => {
  async function snapshot(Provider: AnyComp, Modal: AnyComp, toggled: boolean) {
    const user = userEvent.setup()
    const { unmount } = mount(Provider, Modal)
    if (toggled) await user.click(screen.getByTestId('nf-modal-toggle'))
    const html = screen.getByTestId('nf-modal-backdrop').outerHTML
    unmount()
    return html
  }

  it('initial portal DOM matches between React and PythScribe tracks', async () => {
    const r = await snapshot(ReactProvider, ReactModal, false)
    expect(await snapshot(PsProvider, PsModal, false)).toBe(r)
    expect(await snapshot(PscProvider, PscModal, false)).toBe(r)
  })

  it('post-toggle portal DOM matches between React and PythScribe tracks', async () => {
    const r = await snapshot(ReactProvider, ReactModal, true)
    expect(await snapshot(PsProvider, PsModal, true)).toBe(r)
    expect(await snapshot(PscProvider, PscModal, true)).toBe(r)
  })
})
