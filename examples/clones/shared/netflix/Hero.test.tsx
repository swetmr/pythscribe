// Tri-track render-parity for the Netflix hero banner.
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Hero as ReactHero } from './NetflixApp'
import { Hero as PsHero } from './NetflixApp.ps'
import { Hero as PscHero } from './NetflixApp.psc'
import { netflixTitles, netflixHeroId } from './fixtures'

const hero_title = netflixTitles[netflixHeroId]

type AnyComp = React.ComponentType<any>

function contract(label: string, Hero: AnyComp) {
  describe(label, () => {
    it('renders title, description and meta from props', () => {
      render(<Hero title={hero_title} on_more_info={() => {}} />)
      expect(screen.getByTestId('nf-hero-title')).toHaveTextContent(hero_title.name)
      expect(screen.getByTestId('nf-hero-desc')).toHaveTextContent(hero_title.description)
      expect(screen.getByTestId('nf-hero')).toHaveTextContent(`${hero_title.match}% Match`)
      expect(screen.getByTestId('nf-hero')).toHaveTextContent(hero_title.maturity)
    })

    it('renders Play and More Info CTAs; More Info reports the title', async () => {
      const user = userEvent.setup()
      const on_more_info = vi.fn()
      render(<Hero title={hero_title} on_more_info={on_more_info} />)
      expect(screen.getByTestId('nf-hero-play')).toBeInTheDocument()
      await user.click(screen.getByTestId('nf-hero-info'))
      expect(on_more_info).toHaveBeenCalledTimes(1)
      expect(on_more_info).toHaveBeenCalledWith(hero_title)
    })
  })
}

contract('Hero.tsx (React reference)', ReactHero)
contract('Hero .ps (PythScribe canonical)', PsHero)
contract('Hero .psc (compressed PythScribe)', PscHero)

describe('Hero dual-track DOM parity', () => {
  function snapshot(Hero: AnyComp): string {
    const { container, unmount } = render(<Hero title={hero_title} on_more_info={() => {}} />)
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe tracks', () => {
    const r = snapshot(ReactHero)
    expect(snapshot(PsHero)).toBe(r)
    expect(snapshot(PscHero)).toBe(r)
  })
})
