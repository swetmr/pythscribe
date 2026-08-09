// Tri-track integration parity for the full Netflix clone app: hero + 4
// fixture carousels + live My List row + portal modal, wired through the
// MyListContext provider. The cross-row sync cases lean on fixture titles
// that appear in MORE THAN ONE row (t01 in trending AND acclaimed).
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactNetflixApp from './NetflixApp'
import { NetflixApp as PsNetflixApp } from './NetflixApp.ps'
import { NetflixApp as PscNetflixApp } from './NetflixApp.psc'
import { netflixFixture, netflixTitles, netflixRows } from './fixtures'

type AnyComp = React.ComponentType<any>

function mount(App: AnyComp) {
  return render(<App {...netflixFixture} />)
}

function contract(label: string, App: AnyComp) {
  describe(label, () => {
    it('renders hero + all fixture rows + the empty My List state', () => {
      mount(App)
      expect(screen.getByTestId('nf-hero-title')).toHaveTextContent(
        netflixTitles[netflixFixture.hero_id].name,
      )
      for (const row of netflixRows) {
        expect(screen.getByTestId(`nf-row-${row.id}`)).toBeInTheDocument()
        expect(screen.getByTestId(`nf-track-${row.id}`).children).toHaveLength(
          row.title_ids.length,
        )
      }
      expect(screen.getByTestId('nf-mylist-empty')).toBeInTheDocument()
      expect(screen.queryByTestId('nf-row-my-list')).not.toBeInTheDocument()
    })

    it('My List toggle syncs across carousels and feeds the live My List row', async () => {
      const user = userEvent.setup()
      mount(App)
      // t01 appears in BOTH trending and acclaimed.
      await user.click(screen.getByTestId('nf-toggle-trending-t01'))
      expect(screen.getByTestId('nf-card-trending-t01')).toHaveAttribute('data-in-list', 'true')
      expect(screen.getByTestId('nf-card-acclaimed-t01')).toHaveAttribute('data-in-list', 'true')
      // My List row materializes with the added title.
      expect(screen.getByTestId('nf-row-my-list')).toBeInTheDocument()
      expect(screen.getByTestId('nf-card-my-list-t01')).toBeInTheDocument()
      expect(screen.queryByTestId('nf-mylist-empty')).not.toBeInTheDocument()
      // Add a second title from another row.
      await user.click(screen.getByTestId('nf-toggle-scifi-s01'))
      expect(screen.getByTestId('nf-track-my-list').children).toHaveLength(2)
      // Remove t01 from the OTHER row it appears in — every consumer updates.
      await user.click(screen.getByTestId('nf-toggle-acclaimed-t01'))
      expect(screen.getByTestId('nf-card-trending-t01')).toHaveAttribute('data-in-list', 'false')
      expect(screen.queryByTestId('nf-card-my-list-t01')).not.toBeInTheDocument()
      expect(screen.getByTestId('nf-track-my-list').children).toHaveLength(1)
    })

    it('opens the detail modal from a card, closes on Escape, and the modal toggle updates rows', async () => {
      const user = userEvent.setup()
      mount(App)
      expect(screen.queryByTestId('nf-modal')).not.toBeInTheDocument()
      await user.click(screen.getByTestId('nf-card-trending-t07'))
      expect(screen.getByTestId('nf-modal-title')).toHaveTextContent(netflixTitles['t07'].name)
      expect(screen.getByTestId('nf-modal-backdrop').parentElement).toBe(document.body)
      expect(document.body.style.overflow).toBe('hidden')
      // Toggle from inside the modal — the row card + My List row react.
      await user.click(screen.getByTestId('nf-modal-toggle'))
      expect(screen.getByTestId('nf-card-trending-t07')).toHaveAttribute('data-in-list', 'true')
      expect(screen.getByTestId('nf-card-my-list-t07')).toBeInTheDocument()
      await user.keyboard('{Escape}')
      expect(screen.queryByTestId('nf-modal')).not.toBeInTheDocument()
      expect(document.body.style.overflow).toBe('')
    })

    it('the hero More Info button opens the modal for the hero title', async () => {
      const user = userEvent.setup()
      mount(App)
      await user.click(screen.getByTestId('nf-hero-info'))
      expect(screen.getByTestId('nf-modal-title')).toHaveTextContent(
        netflixTitles[netflixFixture.hero_id].name,
      )
      await user.click(screen.getByTestId('nf-modal-close'))
      expect(screen.queryByTestId('nf-modal')).not.toBeInTheDocument()
    })
  })
}

contract('NetflixApp.tsx (React reference)', ReactNetflixApp)
contract('NetflixApp.ps (PythScribe canonical)', PsNetflixApp)
contract('NetflixApp.psc (compressed PythScribe)', PscNetflixApp)

describe('NetflixApp dual-track DOM parity', () => {
  async function snapshot(App: AnyComp, interact: boolean) {
    const user = userEvent.setup()
    const { container, unmount } = mount(App)
    if (interact) {
      await user.click(screen.getByTestId('nf-toggle-trending-t01'))
      await user.click(screen.getByTestId('nf-card-scifi-s05'))
    }
    // App DOM + portal DOM (modal renders under document.body).
    const html = container.innerHTML
    const portal = screen.queryByTestId('nf-modal-backdrop')?.outerHTML ?? ''
    unmount()
    return html + '\n---portal---\n' + portal
  }

  it('initial DOM matches between React and PythScribe tracks', async () => {
    const r = await snapshot(ReactNetflixApp, false)
    expect(await snapshot(PsNetflixApp, false)).toBe(r)
    expect(await snapshot(PscNetflixApp, false)).toBe(r)
  })

  it('post-interaction DOM (toggle + open modal) matches between tracks', async () => {
    const r = await snapshot(ReactNetflixApp, true)
    expect(await snapshot(PsNetflixApp, true)).toBe(r)
    expect(await snapshot(PscNetflixApp, true)).toBe(r)
  })
})
