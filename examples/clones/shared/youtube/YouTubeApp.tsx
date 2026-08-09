'use client'
import { useState, useEffect, useRef, useMemo } from 'react'
import './YouTubeApp.css'
import type { YtVideo } from './fixtures'

/**
 * React reference oracle for the YouTube clone.
 * Dual-track-paired with YouTubeApp.ps / YouTubeApp.psc — all three must
 * render identical DOM for the same props (see the *.test.tsx parity suites
 * in this directory).
 *
 * SINGLE-MODULE DESIGN (deliberate): all five components live in one
 * tri-track module because `pyths compile` emits extensionless relative
 * imports and scripts/precompile-client.mjs does not rewrite them — a
 * multi-file island's `import './VideoCard'` inside YouTubeApp.client.js
 * would resolve to the `.tsx` ORACLE in the Next client graph (bundler
 * extension order puts .tsx first), silently un-dogfooding the production
 * track. One module per track sidesteps the whole class of problem.
 *
 * Cross-track prop names are snake_case (`on_open`, `on_back`, ...) so the
 * SAME props mount every track in the parity tests (PythScribe component
 * params are snake_case and are not case-converted).
 */

export function fmt_time(t: number): string {
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return s < 10 ? `${m}:0${s}` : `${m}:${s}`
}

export function SearchHeader({ query, on_change }: { query: string; on_change: (q: string) => void }) {
  return (
    <header className="yt-header" data-testid="yt-header">
      <div className="yt-logo">
        <span className="yt-logo-mark">▶</span>
        <span className="yt-logo-text">MyTube</span>
      </div>
      <form className="yt-search-form" onSubmit={(e) => e.preventDefault()}>
        <input
          className="yt-search"
          data-testid="yt-search"
          type="search"
          placeholder="Search"
          value={query}
          onChange={(e) => on_change(e.target.value)}
        />
        <button className="yt-search-btn" type="submit">
          Search
        </button>
      </form>
    </header>
  )
}

export function VideoCard({ item, on_open }: { item: YtVideo; on_open: (v: YtVideo) => void }) {
  const [hovering, set_hovering] = useState(false)
  return (
    <article
      className="yt-card"
      data-testid="yt-card"
      onClick={() => on_open(item)}
      onMouseEnter={() => set_hovering(true)}
      onMouseLeave={() => set_hovering(false)}
    >
      <div className="yt-thumb" style={{ backgroundImage: `url(${item.thumb})`, backgroundSize: 'cover', backgroundColor: item.color }}>
        {hovering ? (
          <video
            className="yt-preview"
            data-testid="yt-preview"
            src="/media/sample.webm"
            autoPlay
            muted
            loop
            playsInline
          />
        ) : (
          <span className="yt-thumb-icon">▶</span>
        )}
        <span className="yt-duration">{item.duration}</span>
      </div>
      <div className="yt-meta">
        <div className="yt-avatar" style={{ background: item.color }}>
          {item.channel[0]}
        </div>
        <div className="yt-info">
          <h3 className="yt-card-title" data-testid="yt-card-title">
            {item.title}
          </h3>
          <p className="yt-channel">{item.channel}</p>
          <p className="yt-stats">{`${item.views} views • ${item.age}`}</p>
        </div>
      </div>
    </article>
  )
}

export function VideoFeed({ videos, on_open }: { videos: YtVideo[]; on_open: (v: YtVideo) => void }) {
  const [visible_count, set_visible_count] = useState(12)
  const sentinel_ref = useRef<HTMLDivElement | null>(null)

  function reset() {
    set_visible_count(12)
  }

  useEffect(reset, [videos])

  function observe() {
    const node = sentinel_ref.current
    if (!node) {
      return
    }
    function on_intersect(entries: IntersectionObserverEntry[]) {
      if (entries[0].isIntersecting) {
        set_visible_count((n) => Math.min(n + 12, videos.length))
      }
    }
    const obs = new IntersectionObserver(on_intersect, { rootMargin: '200px' })
    obs.observe(node)
    return () => obs.disconnect()
  }

  useEffect(observe, [videos])

  const shown = videos.slice(0, visible_count)
  return (
    <section className="yt-feed" data-testid="yt-feed">
      {videos.length === 0 && (
        <p className="yt-empty" data-testid="yt-empty">
          No videos match your search.
        </p>
      )}
      <div className="yt-grid" data-testid="yt-grid">
        {shown.map((v) => (
          <VideoCard key={v.id} item={v} on_open={on_open} />
        ))}
      </div>
      {visible_count < videos.length && (
        <div className="yt-sentinel" data-testid="yt-sentinel" ref={sentinel_ref}>
          Loading more…
        </div>
      )}
    </section>
  )
}

export function WatchView({
  item,
  related,
  subscribed,
  on_toggle_subscribe,
  on_open,
  on_back,
}: {
  item: YtVideo
  related: YtVideo[]
  subscribed: boolean
  on_toggle_subscribe: (channel: string) => void
  on_open: (v: YtVideo) => void
  on_back: () => void
}) {
  const [playing, set_playing] = useState(false)
  const [current_time, set_current_time] = useState(0)
  const [duration, set_duration] = useState(0)
  const video_ref = useRef<HTMLVideoElement | null>(null)

  function toggle_play() {
    const el = video_ref.current
    if (!el) {
      return
    }
    if (playing) {
      el.pause()
      set_playing(false)
    } else {
      const p = el.play()
      if (p && p.catch) {
        p.catch(() => null)
      }
      set_playing(true)
    }
  }

  function seek_by(delta: number) {
    const el = video_ref.current
    if (!el) {
      return
    }
    const d = el.duration || 0
    const t = Math.max(0, Math.min(d, el.currentTime + delta))
    el.currentTime = t
    set_current_time(t)
  }

  function handle_seek(e: { target: { value: string } }) {
    const t = parseFloat(e.target.value)
    const el = video_ref.current
    if (el) {
      el.currentTime = t
    }
    set_current_time(t)
  }

  function handle_key(e: KeyboardEvent) {
    const tag = e.target && (e.target as HTMLElement).tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'BUTTON') {
      return
    }
    if (e.key === ' ' || e.code === 'Space') {
      e.preventDefault()
      toggle_play()
    } else if (e.key === 'ArrowRight') {
      e.preventDefault()
      seek_by(5)
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault()
      seek_by(-5)
    }
  }

  function bind_keys() {
    window.addEventListener('keydown', handle_key)
    return () => window.removeEventListener('keydown', handle_key)
  }

  useEffect(bind_keys, [playing])

  return (
    <section className="yt-watch" data-testid="yt-watch">
      <button className="yt-back" data-testid="yt-back" onClick={() => on_back()}>
        ← Back to feed
      </button>
      <div className="yt-watch-body">
        <div className="yt-primary">
          <video
            className="yt-video"
            data-testid="yt-video"
            ref={video_ref}
            src="/media/sample.webm"
            autoPlay
            muted
            loop
            playsInline
            onTimeUpdate={(e) => set_current_time((e.target as HTMLVideoElement).currentTime)}
            onLoadedMetadata={(e) => set_duration((e.target as HTMLVideoElement).duration || 0)}
            onPlay={() => set_playing(true)}
            onPause={() => set_playing(false)}
          />
          <div className="yt-controls">
            <button className="yt-play" data-testid="yt-play" onClick={() => toggle_play()}>
              {playing ? 'Pause' : 'Play'}
            </button>
            <input
              className="yt-seek"
              data-testid="yt-seek"
              type="range"
              min={0}
              max={duration || 0}
              step={0.1}
              value={current_time}
              onChange={handle_seek}
            />
            <span className="yt-time" data-testid="yt-time">
              {`${fmt_time(current_time)} / ${fmt_time(duration)}`}
            </span>
          </div>
          <h1 className="yt-watch-title" data-testid="yt-watch-title">
            {item.title}
          </h1>
          <div className="yt-channel-row">
            <div className="yt-avatar" style={{ background: item.color }}>
              {item.channel[0]}
            </div>
            <div className="yt-channel-info">
              <p className="yt-channel">{item.channel}</p>
              <p className="yt-stats">{`${item.views} views • ${item.age}`}</p>
            </div>
            <button
              className={'yt-subscribe' + (subscribed ? ' subscribed' : '')}
              data-testid="yt-subscribe"
              onClick={() => on_toggle_subscribe(item.channel)}
            >
              {subscribed ? 'Subscribed' : 'Subscribe'}
            </button>
          </div>
        </div>
        <aside className="yt-related" data-testid="yt-related">
          <h2 className="yt-related-heading">Up next</h2>
          {related.map((v) => (
            <div key={v.id} className="yt-related-item" data-testid="yt-related-item" onClick={() => on_open(v)}>
              <div className="yt-related-thumb" style={{ backgroundImage: `url(${v.thumb})`, backgroundSize: 'cover', backgroundColor: v.color }}>
                <span className="yt-duration">{v.duration}</span>
              </div>
              <div className="yt-related-meta">
                <p className="yt-related-title">{v.title}</p>
                <p className="yt-related-channel">{v.channel}</p>
              </div>
            </div>
          ))}
        </aside>
      </div>
    </section>
  )
}

export function YouTubeApp({ videos }: { videos: YtVideo[] }) {
  const [query, set_query] = useState('')
  const [current, set_current] = useState<YtVideo | null>(null)
  const [subscribed_channels, set_subscribed_channels] = useState<string[]>([])

  const filtered = useMemo(
    () =>
      videos.filter(
        (v) =>
          v['title'].toLowerCase().includes(query.toLowerCase()) ||
          v['channel'].toLowerCase().includes(query.toLowerCase()),
      ),
    [videos, query],
  )

  function handle_query(q: string) {
    set_query(q)
    set_current(null)
  }

  function open_video(v: YtVideo) {
    set_current(v)
  }

  function toggle_subscribe(channel: string) {
    if (subscribed_channels.includes(channel)) {
      set_subscribed_channels(subscribed_channels.filter((c) => c !== channel))
    } else {
      set_subscribed_channels([...subscribed_channels, channel])
    }
  }

  const related = current ? videos.filter((v) => v['id'] !== current['id']).slice(0, 10) : []

  return (
    <div className="yt-app" data-testid="yt-app">
      <SearchHeader query={query} on_change={handle_query} />
      {current ? (
        <WatchView
          key={current.id}
          item={current}
          related={related}
          subscribed={subscribed_channels.includes(current.channel)}
          on_toggle_subscribe={toggle_subscribe}
          on_open={open_video}
          on_back={() => set_current(null)}
        />
      ) : (
        <VideoFeed videos={filtered} on_open={open_video} />
      )}
    </div>
  )
}

export default YouTubeApp
