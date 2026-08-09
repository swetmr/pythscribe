// Tri-track render-parity for the Twitter clone — mounts TwitterApp.tsx
// (React oracle), TwitterApp.ps and TwitterApp.psc with the SAME fixtures
// and asserts equal behavior + equal DOM (see CONTRIBUTING.md).
//
// jsdom has no IntersectionObserver; all three tracks guard on
// window.IntersectionObserver, so the infinite-feed sentinel renders but
// never auto-loads here. Scroll-driven batch loading is covered by
// e2e/twitter.spec.ts in real browsers against both app shells.

import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ReactTwitterApp from './TwitterApp'
import { TwitterApp as PythTwitterApp } from './TwitterApp.ps'
import { TwitterApp as PscTwitterApp } from './TwitterApp.psc'
import { twitterFixture } from './fixtures'

type AppComponent = React.ComponentType<typeof twitterFixture>

function contract(label: string, Component: AppComponent) {
  describe(label, () => {
    it('renders the first batch of 10 tweets with a feed counter', () => {
      render(<Component {...twitterFixture} />)
      expect(screen.getByTestId('twitter-app')).toBeInTheDocument()
      expect(screen.getByTestId('tweet-t1')).toBeInTheDocument()
      expect(screen.getByTestId('tweet-t10')).toBeInTheDocument()
      expect(screen.queryByTestId('tweet-t11')).not.toBeInTheDocument()
      expect(screen.getByTestId('feed-count')).toHaveTextContent('10 of 66')
      // replies never appear in the top-level feed
      expect(screen.queryByTestId('tweet-r1')).not.toBeInTheDocument()
      // sentinel present (more tweets available), guarded no-op in jsdom
      expect(screen.getByTestId('feed-sentinel')).toBeInTheDocument()
    })

    it('renders deterministic relative timestamps from fixture offsets', () => {
      render(<Component {...twitterFixture} />)
      expect(screen.getByTestId('time-t1')).toHaveTextContent('now')
      expect(screen.getByTestId('time-t2')).toHaveTextContent('2m')
      expect(screen.getByTestId('time-t3')).toHaveTextContent('45m')
      expect(screen.getByTestId('time-t4')).toHaveTextContent('3h')
      expect(screen.getByTestId('time-t5')).toHaveTextContent('1d')
      expect(screen.getByTestId('time-t6')).toHaveTextContent('5d')
    })

    it('char counter counts down and over-limit disables the post button', () => {
      render(<Component {...twitterFixture} />)
      const input = screen.getByTestId('compose-input')
      const counter = screen.getByTestId('char-count')
      const post = screen.getByTestId('compose-post')
      expect(counter).toHaveTextContent('280')
      expect(post).toBeDisabled() // empty compose
      fireEvent.change(input, { target: { value: 'hello' } })
      expect(counter).toHaveTextContent('275')
      expect(post).toBeEnabled()
      fireEvent.change(input, { target: { value: 'x'.repeat(281) } })
      expect(counter).toHaveTextContent('-1')
      expect(post).toBeDisabled()
    })

    it('optimistic post: appears instantly as Sending…, then confirms', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      fireEvent.change(screen.getByTestId('compose-input'), {
        target: { value: 'Parity tweet from the suite' },
      })
      await user.click(screen.getByTestId('compose-post'))
      // instant optimistic insert at the top, marked sending
      expect(screen.getByTestId('tweet-local-1')).toBeInTheDocument()
      expect(screen.getByTestId('sending-local-1')).toBeInTheDocument()
      // visible slice stays at 10: the new tweet pushes t10 out of the batch
      expect(screen.getByTestId('feed-count')).toHaveTextContent('10 of 67')
      expect(screen.queryByTestId('tweet-t10')).not.toBeInTheDocument()
      // async send confirms: sending marker goes away, tweet stays
      await waitFor(
        () => expect(screen.queryByTestId('sending-local-1')).not.toBeInTheDocument(),
        { timeout: 3000 },
      )
      expect(screen.getByTestId('tweet-local-1')).toBeInTheDocument()
      expect(screen.queryByTestId('toast')).not.toBeInTheDocument()
    })

    it('optimistic post with "#fail" rolls back with an error toast', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      fireEvent.change(screen.getByTestId('compose-input'), {
        target: { value: 'this one will #fail for sure' },
      })
      await user.click(screen.getByTestId('compose-post'))
      expect(screen.getByTestId('tweet-local-1')).toBeInTheDocument()
      await waitFor(
        () => expect(screen.queryByTestId('tweet-local-1')).not.toBeInTheDocument(),
        { timeout: 3000 },
      )
      expect(screen.getByTestId('toast')).toHaveTextContent('Post failed — rolled back')
    })

    it('like is an optimistic toggle (up, then back down)', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      const like = screen.getByTestId('like-t1')
      expect(like).toHaveTextContent('♡ 12')
      expect(like).toHaveAttribute('data-liked', 'false')
      await user.click(like)
      expect(like).toHaveTextContent('♥ 13')
      expect(like).toHaveAttribute('data-liked', 'true')
      await user.click(like)
      expect(like).toHaveTextContent('♡ 12')
    })

    it('retweet is an optimistic toggle', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      const rt = screen.getByTestId('rt-t1')
      expect(rt).toHaveTextContent('⇄ 4')
      await user.click(rt)
      expect(rt).toHaveTextContent('⇄ 5')
      expect(rt).toHaveAttribute('data-retweeted', 'true')
      await user.click(rt)
      expect(rt).toHaveTextContent('⇄ 4')
    })

    it('liking the "#fail" tweet rolls the counter back with a toast', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      const like = screen.getByTestId('like-t3')
      expect(like).toHaveTextContent('♡ 7')
      await user.click(like)
      expect(like).toHaveTextContent('♥ 8') // optimistic
      await waitFor(() => expect(like).toHaveTextContent('♡ 7'), { timeout: 3000 })
      expect(like).toHaveAttribute('data-liked', 'false')
      expect(screen.getByTestId('toast')).toHaveTextContent('Like failed — rolled back')
    })

    it('opens a thread with replies and navigates back', async () => {
      const user = userEvent.setup()
      render(<Component {...twitterFixture} />)
      expect(screen.getByTestId('replies-t1')).toHaveTextContent('💬 3')
      await user.click(screen.getByTestId('replies-t1'))
      expect(screen.getByTestId('thread')).toBeInTheDocument()
      expect(screen.queryByTestId('feed')).not.toBeInTheDocument()
      expect(screen.getByTestId('tweet-t1')).toBeInTheDocument() // parent
      expect(screen.getByTestId('tweet-r1')).toBeInTheDocument()
      expect(screen.getByTestId('tweet-r2')).toBeInTheDocument()
      expect(screen.getByTestId('tweet-r3')).toBeInTheDocument()
      expect(screen.getByText('Replies (3)')).toBeInTheDocument()
      await user.click(screen.getByTestId('thread-back'))
      expect(screen.getByTestId('feed')).toBeInTheDocument()
      expect(screen.queryByTestId('thread')).not.toBeInTheDocument()
    })
  })
}

contract('TwitterApp.tsx (React reference)', ReactTwitterApp)
contract('TwitterApp.ps (PythScribe canonical)', PythTwitterApp)
contract('TwitterApp.psc (compressed PythScribe)', PscTwitterApp)

describe('Tri-track DOM parity', () => {
  async function snapshot(Component: AppComponent, interact = false): Promise<string> {
    const user = userEvent.setup()
    const { container, unmount } = render(<Component {...twitterFixture} />)
    if (interact) {
      await user.click(screen.getByTestId('like-t1'))
      await user.click(screen.getByTestId('replies-t1'))
    }
    const html = container.innerHTML
    unmount()
    return html
  }

  it('initial DOM matches between React and PythScribe', async () => {
    const r = await snapshot(ReactTwitterApp)
    const p = await snapshot(PythTwitterApp)
    const c = await snapshot(PscTwitterApp)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })

  it('post-interaction DOM (like + thread view) matches between React and PythScribe', async () => {
    const r = await snapshot(ReactTwitterApp, true)
    const p = await snapshot(PythTwitterApp, true)
    const c = await snapshot(PscTwitterApp, true)
    expect(p).toBe(r)
    expect(c).toBe(r)
  })
})
