// Tri-track render-parity for VideoFeed (batched grid + IntersectionObserver
// infinite scroll, driven deterministically via the mock in test-helpers.ts).

import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import { VideoFeed as ReactVideoFeed } from './YouTubeApp'
import { VideoFeed as PsVideoFeed } from './YouTubeApp.ps'
import { VideoFeed as PscVideoFeed } from './YouTubeApp.psc'
import { youtube_videos, type YtVideo } from './fixtures'
import { installIntersectionObserverMock } from './test-helpers'

const io = installIntersectionObserverMock()

beforeEach(() => {
  io.instances.length = 0
})

type Props = { videos: YtVideo[]; on_open: (v: YtVideo) => void }

function lastObserver() {
  expect(io.instances.length).toBeGreaterThan(0)
  return io.instances[io.instances.length - 1]
}

function contract(label: string, Component: React.ComponentType<Props>) {
  describe(label, () => {
    it('renders the first batch of 12 cards plus a sentinel', () => {
      render(<Component videos={youtube_videos} on_open={() => {}} />)
      expect(screen.getAllByTestId('yt-card')).toHaveLength(12)
      expect(screen.getByTestId('yt-sentinel')).toBeInTheDocument()
      expect(lastObserver().observed).toContain(screen.getByTestId('yt-sentinel'))
    })

    it('appends the next batch when the sentinel intersects', () => {
      render(<Component videos={youtube_videos} on_open={() => {}} />)
      act(() => lastObserver().trigger(true))
      expect(screen.getAllByTestId('yt-card')).toHaveLength(24)
      act(() => lastObserver().trigger(true))
      expect(screen.getAllByTestId('yt-card')).toHaveLength(36)
    })

    it('does not append on a non-intersecting notification', () => {
      render(<Component videos={youtube_videos} on_open={() => {}} />)
      act(() => lastObserver().trigger(false))
      expect(screen.getAllByTestId('yt-card')).toHaveLength(12)
    })

    it('removes the sentinel once every video is shown and clamps at the end', () => {
      render(<Component videos={youtube_videos.slice(0, 20)} on_open={() => {}} />)
      act(() => lastObserver().trigger(true))
      expect(screen.getAllByTestId('yt-card')).toHaveLength(20)
      expect(screen.queryByTestId('yt-sentinel')).not.toBeInTheDocument()
    })

    it('renders no sentinel when the list fits in one batch, and an empty state for zero videos', () => {
      const { unmount } = render(<Component videos={youtube_videos.slice(0, 5)} on_open={() => {}} />)
      expect(screen.getAllByTestId('yt-card')).toHaveLength(5)
      expect(screen.queryByTestId('yt-sentinel')).not.toBeInTheDocument()
      unmount()
      render(<Component videos={[]} on_open={() => {}} />)
      expect(screen.getByTestId('yt-empty')).toHaveTextContent('No videos match your search.')
    })
  })
}

contract('VideoFeed.tsx (React reference)', ReactVideoFeed)
contract('VideoFeed.ps (PythScribe canonical)', PsVideoFeed)
contract('VideoFeed.psc (compressed PythScribe)', PscVideoFeed)

describe('VideoFeed dual-track DOM parity', () => {
  async function snapshot(Component: React.ComponentType<Props>, scroll = false) {
    io.instances.length = 0
    const { container, unmount } = render(<Component videos={youtube_videos} on_open={() => {}} />)
    if (scroll) act(() => io.instances[io.instances.length - 1].trigger(true))
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactVideoFeed)
    const p = await snapshot(PsVideoFeed)
    const c = await snapshot(PscVideoFeed)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-infinite-scroll DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactVideoFeed, true)
    const p = await snapshot(PsVideoFeed, true)
    const c = await snapshot(PscVideoFeed, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
