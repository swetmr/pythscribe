// Tri-track render-parity for the Carousel row: renders every card, and the
// left/right buttons drive el.scrollBy on the track ref (jsdom has no
// scrollBy, so it is stubbed with a spy — the wiring is what's under test;
// real scrolling is covered by e2e/netflix.spec.ts in both apps).
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Carousel as ReactCarousel, MyListProvider as ReactProvider } from './NetflixApp'
import { Carousel as PsCarousel, MyListProvider as PsProvider } from './NetflixApp.ps'
import { Carousel as PscCarousel, MyListProvider as PscProvider } from './NetflixApp.psc'
import { netflixTitles, netflixRows } from './fixtures'

const row = netflixRows[0] // trending, 12 titles
const titles = row.title_ids.map((tid) => netflixTitles[tid])

type AnyComp = React.ComponentType<any>

const scroll_spy = vi.fn()
beforeEach(() => {
  scroll_spy.mockClear()
  ;(Element.prototype as any).scrollBy = scroll_spy
})
afterEach(() => {
  delete (Element.prototype as any).scrollBy
})

function mount(Provider: AnyComp, Carousel: AnyComp) {
  return render(
    <Provider>
      <Carousel row_id={row.id} label={row.label} titles={titles} on_open={() => {}} />
    </Provider>,
  )
}

function contract(label: string, Provider: AnyComp, Carousel: AnyComp) {
  describe(label, () => {
    it('renders the row label and all cards in the track', () => {
      mount(Provider, Carousel)
      expect(screen.getByTestId(`nf-row-${row.id}`)).toHaveTextContent(row.label)
      const track = screen.getByTestId(`nf-track-${row.id}`)
      expect(track.children).toHaveLength(titles.length)
      for (const t of titles) {
        expect(screen.getByTestId(`nf-card-${row.id}-${t.id}`)).toBeInTheDocument()
      }
    })

    it('right/left buttons scroll the track by +/-600', async () => {
      const user = userEvent.setup()
      mount(Provider, Carousel)
      await user.click(screen.getByTestId(`nf-scroll-right-${row.id}`))
      expect(scroll_spy).toHaveBeenCalledWith({ left: 600, behavior: 'smooth' })
      await user.click(screen.getByTestId(`nf-scroll-left-${row.id}`))
      expect(scroll_spy).toHaveBeenCalledWith({ left: -600, behavior: 'smooth' })
    })
  })
}

contract('Carousel.tsx (React reference)', ReactProvider, ReactCarousel)
contract('Carousel .ps (PythScribe canonical)', PsProvider, PsCarousel)
contract('Carousel .psc (compressed PythScribe)', PscProvider, PscCarousel)

describe('Carousel dual-track DOM parity', () => {
  function snapshot(Provider: AnyComp, Carousel: AnyComp): string {
    const { container, unmount } = mount(Provider, Carousel)
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe tracks', () => {
    const r = snapshot(ReactProvider, ReactCarousel)
    expect(snapshot(PsProvider, PsCarousel)).toBe(r)
    expect(snapshot(PscProvider, PscCarousel)).toBe(r)
  })
})
