// Tri-track render-parity for WatchView (player controls, seek binding,
// keyboard shortcuts, subscribe, related rail).
//
// jsdom note: HTMLMediaElement.play()/pause() are not implemented in jsdom
// (they log a jsdomError and return undefined) — the component guards for
// that, and playing state is component state, so the behavioral contract is
// still assertable. currentTime/duration are overridden per-element with
// Object.defineProperty where a test needs real numbers.

import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WatchView as ReactWatchView } from './YouTubeApp'
import { WatchView as PsWatchView } from './YouTubeApp.ps'
import { WatchView as PscWatchView } from './YouTubeApp.psc'
import { youtube_videos, type YtVideo } from './fixtures'

const item = youtube_videos[0]
const related = youtube_videos.slice(1, 11)

type Props = {
  item: YtVideo
  related: YtVideo[]
  subscribed: boolean
  on_toggle_subscribe: (channel: string) => void
  on_open: (v: YtVideo) => void
  on_back: () => void
}

function renderView(Component: React.ComponentType<Props>, overrides: Partial<Props> = {}) {
  const props: Props = {
    item,
    related,
    subscribed: false,
    on_toggle_subscribe: () => {},
    on_open: () => {},
    on_back: () => {},
    ...overrides,
  }
  return render(<Component {...props} />)
}

function contract(label: string, Component: React.ComponentType<Props>) {
  describe(label, () => {
    it('renders the video element, title, channel row and 10 related items', () => {
      renderView(Component)
      expect(screen.getByTestId('yt-video')).toHaveAttribute('src', '/media/sample.webm')
      expect(screen.getByTestId('yt-watch-title')).toHaveTextContent(item.title)
      expect(screen.getByTestId('yt-subscribe')).toHaveTextContent('Subscribe')
      expect(screen.getAllByTestId('yt-related-item')).toHaveLength(10)
      expect(screen.getByTestId('yt-time')).toHaveTextContent('0:00 / 0:00')
    })

    it('toggles the play/pause button state on click', async () => {
      const user = userEvent.setup()
      renderView(Component)
      const btn = screen.getByTestId('yt-play')
      expect(btn).toHaveTextContent('Play')
      await user.click(btn)
      expect(btn).toHaveTextContent('Pause')
      await user.click(btn)
      expect(btn).toHaveTextContent('Play')
    })

    it('binds the seek bar + time display to timeUpdate/loadedMetadata', () => {
      renderView(Component)
      const video = screen.getByTestId('yt-video') as HTMLVideoElement
      Object.defineProperty(video, 'duration', { value: 120, configurable: true })
      Object.defineProperty(video, 'currentTime', { value: 65, writable: true, configurable: true })
      fireEvent.loadedMetadata(video)
      fireEvent.timeUpdate(video)
      expect(screen.getByTestId('yt-time')).toHaveTextContent('1:05 / 2:00')
      expect(screen.getByTestId('yt-seek')).toHaveValue('65')
    })

    it('seeks via the range input and writes through to the video element', () => {
      renderView(Component)
      const video = screen.getByTestId('yt-video') as HTMLVideoElement
      Object.defineProperty(video, 'duration', { value: 120, configurable: true })
      Object.defineProperty(video, 'currentTime', { value: 0, writable: true, configurable: true })
      fireEvent.loadedMetadata(video)
      fireEvent.change(screen.getByTestId('yt-seek'), { target: { value: '30' } })
      expect(video.currentTime).toBe(30)
      expect(screen.getByTestId('yt-time')).toHaveTextContent('0:30 / 2:00')
    })

    it('space key toggles play/pause; arrow keys seek and clamp', () => {
      renderView(Component)
      const video = screen.getByTestId('yt-video') as HTMLVideoElement
      Object.defineProperty(video, 'duration', { value: 120, configurable: true })
      Object.defineProperty(video, 'currentTime', { value: 0, writable: true, configurable: true })
      const btn = screen.getByTestId('yt-play')
      fireEvent.keyDown(window, { key: ' ', code: 'Space' })
      expect(btn).toHaveTextContent('Pause')
      fireEvent.keyDown(window, { key: ' ', code: 'Space' })
      expect(btn).toHaveTextContent('Play')
      fireEvent.keyDown(window, { key: 'ArrowRight' })
      expect(video.currentTime).toBe(5)
      fireEvent.keyDown(window, { key: 'ArrowLeft' })
      fireEvent.keyDown(window, { key: 'ArrowLeft' })
      expect(video.currentTime).toBe(0)
    })

    it('calls on_toggle_subscribe with the channel; renders Subscribed when subscribed', async () => {
      const user = userEvent.setup()
      const on_toggle_subscribe = vi.fn()
      const { unmount } = renderView(Component, { on_toggle_subscribe })
      await user.click(screen.getByTestId('yt-subscribe'))
      expect(on_toggle_subscribe).toHaveBeenCalledWith(item.channel)
      unmount()
      renderView(Component, { subscribed: true })
      expect(screen.getByTestId('yt-subscribe')).toHaveTextContent('Subscribed')
    })

    it('calls on_open from the related rail and on_back from the back button', async () => {
      const user = userEvent.setup()
      const on_open = vi.fn()
      const on_back = vi.fn()
      renderView(Component, { on_open, on_back })
      await user.click(screen.getAllByTestId('yt-related-item')[0])
      expect(on_open).toHaveBeenCalledWith(related[0])
      await user.click(screen.getByTestId('yt-back'))
      expect(on_back).toHaveBeenCalled()
    })
  })
}

contract('WatchView.tsx (React reference)', ReactWatchView)
contract('WatchView.ps (PythScribe canonical)', PsWatchView)
contract('WatchView.psc (compressed PythScribe)', PscWatchView)

describe('WatchView dual-track DOM parity', () => {
  async function snapshot(Component: React.ComponentType<Props>, play = false) {
    const user = userEvent.setup()
    const { container, unmount } = renderView(Component)
    if (play) await user.click(screen.getByTestId('yt-play'))
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactWatchView)
    const p = await snapshot(PsWatchView)
    const c = await snapshot(PscWatchView)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-play DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactWatchView, true)
    const p = await snapshot(PsWatchView, true)
    const c = await snapshot(PscWatchView, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
