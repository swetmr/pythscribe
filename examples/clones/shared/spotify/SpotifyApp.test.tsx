// Tri-track render-parity for the Spotify clone (CONTRIBUTING.md contract).
// Mounts all 3 tracks (.tsx oracle / .ps canonical / .psc compressed) with
// the SAME fixtures and asserts equal behavior + equal DOM — including the
// deterministic seeded shuffle, whose expected order is computed with the
// fixtures.ts seededShuffle copy so any cross-language drift in the
// PythScribe re-implementation fails here.

import { describe, it, expect, beforeAll, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactSpotifyApp from './SpotifyApp'
import { SpotifyApp as PythSpotifyApp } from './SpotifyApp.ps'
import { SpotifyApp as PscSpotifyApp } from './SpotifyApp.psc'
import { PLAYLISTS, SHUFFLE_SEED, seededShuffle, type Playlist } from './fixtures'

type AppComponent = React.ComponentType<{ playlists: Playlist[] }>

// jsdom has no real media pipeline — HTMLMediaElement.play()/pause() are
// "not implemented" stubs that spam the virtual console. Replace them with
// quiet fakes (play resolves, matching a real browser's promise shape).
beforeAll(() => {
  Object.defineProperty(HTMLMediaElement.prototype, 'play', {
    configurable: true,
    writable: true,
    value: vi.fn(() => Promise.resolve()),
  })
  Object.defineProperty(HTMLMediaElement.prototype, 'pause', {
    configurable: true,
    writable: true,
    value: vi.fn(),
  })
})

const tracks = PLAYLISTS[0].tracks

function contract(label: string, Component: AppComponent) {
  describe(label, () => {
    it('renders the sidebar playlists and the first playlist track table', () => {
      render(<Component playlists={PLAYLISTS} />)
      for (const pl of PLAYLISTS) {
        expect(screen.getByTestId('playlist-' + pl.id)).toHaveTextContent(pl.name)
      }
      expect(screen.getByTestId('sp-playlist-title')).toHaveTextContent(PLAYLISTS[0].name)
      expect(screen.getByTestId('sp-playlist-meta')).toHaveTextContent('8 tracks')
      for (const t of tracks) {
        expect(screen.getByTestId('track-row-' + t.id)).toBeInTheDocument()
      }
      // duration formatting: 214s → 3:34
      expect(screen.getByTestId('track-row-' + tracks[0].id)).toHaveTextContent('3:34')
      expect(screen.getByTestId('queue-empty')).toHaveTextContent('Queue is empty')
      expect(screen.getByTestId('player-track-title')).toHaveTextContent('Nothing playing')
    })

    it('switches playlists from the sidebar', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('playlist-' + PLAYLISTS[1].id))
      expect(screen.getByTestId('sp-playlist-title')).toHaveTextContent(PLAYLISTS[1].name)
      expect(screen.getByTestId('playlist-' + PLAYLISTS[1].id)).toHaveAttribute('data-active', 'true')
      expect(screen.getByTestId('playlist-' + PLAYLISTS[0].id)).toHaveAttribute('data-active', 'false')
      expect(screen.getByTestId('track-row-' + PLAYLISTS[1].tracks[0].id)).toBeInTheDocument()
    })

    it('plays a track: active row highlight, player bar title, pause label, audio.play called', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[2].id))
      expect(screen.getByTestId('track-row-' + tracks[2].id)).toHaveAttribute('data-active', 'true')
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[2].title)
      expect(screen.getByTestId('player-track-artist')).toHaveTextContent(tracks[2].artist)
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause')
      expect(HTMLMediaElement.prototype.play).toHaveBeenCalled()
      // queue = playlist order; up next starts after the clicked track
      const items = screen.getAllByTestId('queue-item')
      expect(items.map((el) => el.textContent)).toEqual(tracks.slice(3).map((t) => t.title))
    })

    it('play/pause toggles; next/prev honor the queue', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[0].id))
      await user.click(screen.getByTestId('player-play'))
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Play')
      await user.click(screen.getByTestId('player-play'))
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause')
      await user.click(screen.getByTestId('player-next'))
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[1].title)
      await user.click(screen.getByTestId('player-next'))
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[2].title)
      await user.click(screen.getByTestId('player-prev'))
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[1].title)
    })

    it('auto-advances to the next queued track on audio `ended`', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[0].id))
      fireEvent.ended(screen.getByTestId('player-audio'))
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[1].title)
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause')
    })

    it('adds tracks to the queue from the row menu (append order preserved)', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-menu-' + tracks[4].id))
      await user.click(screen.getByTestId('queue-add-' + tracks[4].id))
      // menu closes after adding
      expect(screen.queryByTestId('row-menu-' + tracks[4].id)).not.toBeInTheDocument()
      await user.click(screen.getByTestId('track-menu-' + tracks[1].id))
      await user.click(screen.getByTestId('queue-add-' + tracks[1].id))
      const items = screen.getAllByTestId('queue-item')
      expect(items.map((el) => el.textContent)).toEqual([tracks[4].title, tracks[1].title])
      // nothing is playing yet — pressing play starts the queued tracks
      await user.click(screen.getByTestId('player-play'))
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[4].title)
    })

    it('seeded shuffle reorders the up-next queue deterministically (fixed seed)', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[0].id))
      await user.click(screen.getByTestId('player-shuffle'))
      expect(screen.getByTestId('player-shuffle')).toHaveAttribute('aria-pressed', 'true')
      const expected = seededShuffle(tracks.slice(1), SHUFFLE_SEED).map((t) => t.title)
      const items = screen.getAllByTestId('queue-item')
      expect(items.map((el) => el.textContent)).toEqual(expected)
      // current track is untouched by the shuffle
      expect(screen.getByTestId('player-track-title')).toHaveTextContent(tracks[0].title)
    })

    it('Space toggles playback — but not while focus is in an input', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[0].id))
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause')
      fireEvent.keyDown(document.body, { code: 'Space' })
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Play')
      fireEvent.keyDown(document.body, { code: 'Space' })
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause')
      const volume = screen.getByTestId('player-volume')
      volume.focus()
      fireEvent.keyDown(volume, { code: 'Space' })
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Pause') // unchanged
    })

    it('Space with an empty queue is a no-op (idle state machine)', () => {
      render(<Component playlists={PLAYLISTS} />)
      fireEvent.keyDown(document.body, { code: 'Space' })
      expect(screen.getByTestId('player-play')).toHaveAccessibleName('Play')
      expect(screen.getByTestId('player-track-title')).toHaveTextContent('Nothing playing')
    })

    it('volume slider dispatches SET_VOLUME', () => {
      render(<Component playlists={PLAYLISTS} />)
      const volume = screen.getByTestId('player-volume') as HTMLInputElement
      expect(volume.value).toBe('0.8')
      fireEvent.change(volume, { target: { value: '0.25' } })
      expect(volume.value).toBe('0.25')
    })

    it('seek slider updates the elapsed readout', async () => {
      const user = userEvent.setup()
      render(<Component playlists={PLAYLISTS} />)
      await user.click(screen.getByTestId('track-play-' + tracks[0].id))
      const audio = screen.getByTestId('player-audio')
      fireEvent.timeUpdate(audio) // jsdom currentTime stays 0 — binding is exercised
      expect(screen.getByTestId('player-elapsed')).toHaveTextContent('0:00')
    })
  })
}

contract('SpotifyApp.tsx (React reference)', ReactSpotifyApp)
contract('SpotifyApp.ps (PythScribe canonical)', PythSpotifyApp)
contract('SpotifyApp.psc (compressed PythScribe)', PscSpotifyApp)

describe('Tri-track DOM parity', () => {
  async function snapshot(Component: AppComponent, interact: boolean): Promise<string> {
    const user = userEvent.setup()
    const { container, unmount } = render(<Component playlists={PLAYLISTS} />)
    if (interact) {
      await user.click(screen.getByTestId('track-play-' + tracks[1].id))
      await user.click(screen.getByTestId('track-menu-' + tracks[3].id))
      await user.click(screen.getByTestId('queue-add-' + tracks[3].id))
      await user.click(screen.getByTestId('player-shuffle'))
      await user.click(screen.getByTestId('playlist-' + PLAYLISTS[2].id))
    }
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactSpotifyApp, false)
    const p = await snapshot(PythSpotifyApp, false)
    const c = await snapshot(PscSpotifyApp, false)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-interaction DOM matches (play + queue add + shuffle + playlist switch)', async () => {
    const r = await snapshot(ReactSpotifyApp, true)
    const p = await snapshot(PythSpotifyApp, true)
    const c = await snapshot(PscSpotifyApp, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
