// Tri-track render-parity for YouTubeApp (top-level composition: search
// filtering, feed <-> watch client-side view switch, app-held subscribe
// state, related-video navigation).

import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactYouTubeApp from './YouTubeApp'
import { YouTubeApp as PsYouTubeApp } from './YouTubeApp.ps'
import { YouTubeApp as PscYouTubeApp } from './YouTubeApp.psc'
import { youtube_videos, type YtVideo } from './fixtures'
import { installIntersectionObserverMock } from './test-helpers'

const io = installIntersectionObserverMock()

beforeEach(() => {
  io.instances.length = 0
})

type Props = { videos: YtVideo[] }

function contract(label: string, Component: React.ComponentType<Props>) {
  describe(label, () => {
    it('renders the header and the first feed batch', () => {
      render(<Component videos={youtube_videos} />)
      expect(screen.getByTestId('yt-search')).toBeInTheDocument()
      expect(screen.getAllByTestId('yt-card')).toHaveLength(12)
      expect(screen.queryByTestId('yt-watch')).not.toBeInTheDocument()
    })

    it('search filters the feed; nonsense query shows the empty state', async () => {
      const user = userEvent.setup()
      render(<Component videos={youtube_videos} />)
      const input = screen.getByTestId('yt-search')
      await user.type(input, 'ramen')
      const cards = screen.getAllByTestId('yt-card')
      expect(cards).toHaveLength(4)
      for (const title of screen.getAllByTestId('yt-card-title')) {
        expect(title.textContent!.toLowerCase()).toContain('ramen')
      }
      await user.clear(input)
      expect(screen.getAllByTestId('yt-card')).toHaveLength(12)
      await user.type(input, 'zzzzzz')
      expect(screen.queryAllByTestId('yt-card')).toHaveLength(0)
      expect(screen.getByTestId('yt-empty')).toBeInTheDocument()
    })

    it('clicking a card switches to the watch view (no route change) and back returns to the feed', async () => {
      const user = userEvent.setup()
      render(<Component videos={youtube_videos} />)
      await user.click(screen.getAllByTestId('yt-card')[0])
      expect(screen.getByTestId('yt-watch')).toBeInTheDocument()
      expect(screen.getByTestId('yt-watch-title')).toHaveTextContent(youtube_videos[0].title)
      expect(screen.getAllByTestId('yt-related-item')).toHaveLength(10)
      await user.click(screen.getByTestId('yt-back'))
      expect(screen.queryByTestId('yt-watch')).not.toBeInTheDocument()
      expect(screen.getAllByTestId('yt-card')).toHaveLength(12)
    })

    it('subscribe state is held by the app and toggles the button text', async () => {
      const user = userEvent.setup()
      render(<Component videos={youtube_videos} />)
      await user.click(screen.getAllByTestId('yt-card')[0])
      const btn = screen.getByTestId('yt-subscribe')
      expect(btn).toHaveTextContent('Subscribe')
      await user.click(btn)
      expect(btn).toHaveTextContent('Subscribed')
      await user.click(btn)
      expect(btn).toHaveTextContent('Subscribe')
    })

    it('clicking a related video switches the watch view to it', async () => {
      const user = userEvent.setup()
      render(<Component videos={youtube_videos} />)
      await user.click(screen.getAllByTestId('yt-card')[0])
      await user.click(screen.getAllByTestId('yt-related-item')[0])
      expect(screen.getByTestId('yt-watch-title')).toHaveTextContent(youtube_videos[1].title)
    })

    it('infinite scroll appends the next batch inside the app', () => {
      render(<Component videos={youtube_videos} />)
      act(() => io.instances[io.instances.length - 1].trigger(true))
      expect(screen.getAllByTestId('yt-card')).toHaveLength(24)
    })
  })
}

contract('YouTubeApp.tsx (React reference)', ReactYouTubeApp)
contract('YouTubeApp.ps (PythScribe canonical)', PsYouTubeApp)
contract('YouTubeApp.psc (compressed PythScribe)', PscYouTubeApp)

describe('YouTubeApp dual-track DOM parity', () => {
  async function snapshot(Component: React.ComponentType<Props>, open = false) {
    io.instances.length = 0
    const user = userEvent.setup()
    const { container, unmount } = render(<Component videos={youtube_videos} />)
    if (open) await user.click(screen.getAllByTestId('yt-card')[0])
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactYouTubeApp)
    const p = await snapshot(PsYouTubeApp)
    const c = await snapshot(PscYouTubeApp)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('watch-view DOM (after opening a video) matches between React and PythScribe', async () => {
    const r = await snapshot(ReactYouTubeApp, true)
    const p = await snapshot(PsYouTubeApp, true)
    const c = await snapshot(PscYouTubeApp, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
