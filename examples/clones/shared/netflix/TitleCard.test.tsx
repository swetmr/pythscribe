// Tri-track render-parity for TitleCard: hover-preview info, My List toggle
// (context consumer), stopPropagation between the toggle and the card's
// open-modal click. Each track is mounted inside ITS OWN track's
// MyListProvider so context wiring is exercised per track.
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import {
  TitleCard as ReactTitleCard,
  MyListProvider as ReactProvider,
} from './NetflixApp'
import { TitleCard as PsTitleCard, MyListProvider as PsProvider } from './NetflixApp.ps'
import { TitleCard as PscTitleCard, MyListProvider as PscProvider } from './NetflixApp.psc'
import { netflixTitles } from './fixtures'

const title = netflixTitles['t04']

type AnyComp = React.ComponentType<any>

function mount(Provider: AnyComp, TitleCard: AnyComp, on_open: (t: unknown) => void = () => {}) {
  return render(
    <Provider>
      <TitleCard row_id="trending" title={title} on_open={on_open} />
    </Provider>,
  )
}

function contract(label: string, Provider: AnyComp, TitleCard: AnyComp) {
  describe(label, () => {
    it('renders name and preview info', () => {
      mount(Provider, TitleCard)
      expect(screen.getByTestId('nf-card-trending-t04')).toHaveTextContent(title.name)
      expect(screen.getByTestId('nf-preview-trending-t04')).toHaveTextContent(
        `${title.match}% Match`,
      )
      expect(screen.getByTestId('nf-preview-trending-t04')).toHaveTextContent(
        `${title.year} | ${title.duration}`,
      )
    })

    it('toggles My List membership on the +/check button', async () => {
      const user = userEvent.setup()
      mount(Provider, TitleCard)
      const card = screen.getByTestId('nf-card-trending-t04')
      const toggle = screen.getByTestId('nf-toggle-trending-t04')
      expect(card).toHaveAttribute('data-in-list', 'false')
      expect(toggle).toHaveTextContent('+')
      expect(toggle).toHaveAttribute('aria-label', `Add ${title.name} to My List`)
      await user.click(toggle)
      expect(card).toHaveAttribute('data-in-list', 'true')
      expect(toggle).toHaveTextContent('✓')
      expect(toggle).toHaveAttribute('aria-label', `Remove ${title.name} from My List`)
      await user.click(toggle)
      expect(card).toHaveAttribute('data-in-list', 'false')
    })

    it('clicking the card opens it; clicking the toggle does NOT (stopPropagation)', async () => {
      const user = userEvent.setup()
      const on_open = vi.fn()
      mount(Provider, TitleCard, on_open)
      await user.click(screen.getByTestId('nf-toggle-trending-t04'))
      expect(on_open).not.toHaveBeenCalled()
      await user.click(screen.getByTestId('nf-card-trending-t04'))
      expect(on_open).toHaveBeenCalledTimes(1)
      expect(on_open).toHaveBeenCalledWith(title)
    })
  })
}

contract('TitleCard.tsx (React reference)', ReactProvider, ReactTitleCard)
contract('TitleCard .ps (PythScribe canonical)', PsProvider, PsTitleCard)
contract('TitleCard .psc (compressed PythScribe)', PscProvider, PscTitleCard)

describe('TitleCard dual-track DOM parity', () => {
  async function snapshot(Provider: AnyComp, TitleCard: AnyComp, toggled: boolean) {
    const user = userEvent.setup()
    const { container, unmount } = mount(Provider, TitleCard)
    if (toggled) await user.click(screen.getByTestId('nf-toggle-trending-t04'))
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe tracks', async () => {
    const r = await snapshot(ReactProvider, ReactTitleCard, false)
    expect(await snapshot(PsProvider, PsTitleCard, false)).toBe(r)
    expect(await snapshot(PscProvider, PscTitleCard, false)).toBe(r)
  })

  it('post-toggle DOM matches between React and PythScribe tracks', async () => {
    const r = await snapshot(ReactProvider, ReactTitleCard, true)
    expect(await snapshot(PsProvider, PsTitleCard, true)).toBe(r)
    expect(await snapshot(PscProvider, PscTitleCard, true)).toBe(r)
  })
})
