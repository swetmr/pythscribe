// Tri-track render-parity for VideoCard (hover-preview + open callback).

import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { VideoCard as ReactVideoCard } from './YouTubeApp'
import { VideoCard as PsVideoCard } from './YouTubeApp.ps'
import { VideoCard as PscVideoCard } from './YouTubeApp.psc'
import { youtube_videos, type YtVideo } from './fixtures'

const item = youtube_videos[0]

type Props = { item: YtVideo; on_open: (v: YtVideo) => void }

function contract(label: string, Component: React.ComponentType<Props>) {
  describe(label, () => {
    it('renders title, channel, stats and duration badge from the fixture', () => {
      render(<Component item={item} on_open={() => {}} />)
      expect(screen.getByTestId('yt-card-title')).toHaveTextContent(item.title)
      expect(screen.getByTestId('yt-card')).toHaveTextContent(item.channel)
      expect(screen.getByTestId('yt-card')).toHaveTextContent(`${item.views} views • ${item.age}`)
      expect(screen.getByTestId('yt-card')).toHaveTextContent(item.duration)
    })

    it('calls on_open with the video on click', async () => {
      const user = userEvent.setup()
      const on_open = vi.fn()
      render(<Component item={item} on_open={on_open} />)
      await user.click(screen.getByTestId('yt-card'))
      expect(on_open).toHaveBeenCalledWith(item)
    })

    it('shows an autoplaying video preview on hover and removes it on unhover', async () => {
      const user = userEvent.setup()
      render(<Component item={item} on_open={() => {}} />)
      expect(screen.queryByTestId('yt-preview')).not.toBeInTheDocument()
      await user.hover(screen.getByTestId('yt-card'))
      const preview = screen.getByTestId('yt-preview')
      expect(preview).toBeInTheDocument()
      expect(preview).toHaveAttribute('src', '/media/sample.webm')
      await user.unhover(screen.getByTestId('yt-card'))
      expect(screen.queryByTestId('yt-preview')).not.toBeInTheDocument()
    })
  })
}

contract('VideoCard.tsx (React reference)', ReactVideoCard)
contract('VideoCard.ps (PythScribe canonical)', PsVideoCard)
contract('VideoCard.psc (compressed PythScribe)', PscVideoCard)

describe('VideoCard dual-track DOM parity', () => {
  async function snapshot(Component: React.ComponentType<Props>, hover = false) {
    const user = userEvent.setup()
    const { container, unmount } = render(<Component item={item} on_open={() => {}} />)
    if (hover) await user.hover(screen.getByTestId('yt-card'))
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactVideoCard)
    const p = await snapshot(PsVideoCard)
    const c = await snapshot(PscVideoCard)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('hovered (preview shown) DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactVideoCard, true)
    const p = await snapshot(PsVideoCard, true)
    const c = await snapshot(PscVideoCard, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
