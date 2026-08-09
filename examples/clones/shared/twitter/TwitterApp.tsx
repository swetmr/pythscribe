'use client'
import { useEffect, useRef, useState } from 'react'
import './TwitterApp.css'

/**
 * React reference oracle for the Twitter clone ("Chirper").
 * Dual-track-paired with TwitterApp.ps / TwitterApp.psc — all three must
 * render identical DOM for the same props (see TwitterApp.test.tsx).
 *
 * Stress-test surface: IntersectionObserver infinite feed, optimistic
 * compose (appears "Sending…" → confirms; "#fail" text rolls back with a
 * toast), optimistic like/retweet with the same "#fail" rollback
 * convention, client-side thread view, deterministic relative timestamps
 * (fixture offsets against a fixed now_ts prop — the .ps track computes
 * these with the Python `datetime` stdlib module, deliberately).
 */

export interface TwitterUser {
  name: string
  handle: string
}

export interface Tweet {
  id: string
  author: TwitterUser
  text: string
  ts: number
  likes: number
  retweets: number
  replyTo: string | null
  pending?: boolean
}

interface Effective {
  liked: boolean
  retweeted: boolean
  likes: number
  retweets: number
}

const BATCH = 10
const SEND_MS = 600
const TOAST_MS = 3000

// Mirrors relative_time in TwitterApp.ps (which uses the Python `datetime`
// stdlib: datetime.fromtimestamp subtraction → timedelta). Same outputs.
export function relative_time(created_ts: number, now_ts: number): string {
  const secs = now_ts - created_ts
  if (secs < 60) return 'now'
  if (secs < 3600) return `${Math.trunc(secs / 60)}m`
  if (secs < 86400) return `${Math.trunc(secs / 3600)}h`
  return `${Math.trunc(secs / 86400)}d`
}

function initials(name: string): string {
  return name
    .split(' ')
    .map((w) => w[0])
    .join('')
}

function sleep_ms(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

// Mirrors PostFailedError (a Tier-7 custom exception class) in TwitterApp.ps.
export class PostFailedError extends Error {
  text: string
  constructor(text: string) {
    super(`post failed: ${text}`)
    this.text = text
  }
}

// Simulated async send: deterministic failure when the text contains "#fail".
async function send_tweet(text: string): Promise<boolean> {
  await sleep_ms(SEND_MS)
  if (text.includes('#fail')) {
    throw new PostFailedError(text)
  }
  return true
}

function TweetCard({
  t,
  eff,
  reply_count,
  now_ts,
  on_open,
  on_like,
  on_rt,
}: {
  t: Tweet
  eff: Effective
  reply_count: number
  now_ts: number
  on_open: (t: Tweet) => void
  on_like: (t: Tweet) => void
  on_rt: (t: Tweet) => void
}) {
  return (
    <article className="tw-tweet" data-testid={`tweet-${t.id}`}>
      <div className="tw-avatar">{initials(t.author.name)}</div>
      <div className="tw-tweet-main">
        <div className="tw-meta">
          <span className="tw-name">{t.author.name}</span>
          <span className="tw-handle">@{t.author.handle}</span>
          <span className="tw-dot">·</span>
          <span className="tw-time" data-testid={`time-${t.id}`}>
            {relative_time(t.ts, now_ts)}
          </span>
          {t.pending && (
            <span className="tw-sending" data-testid={`sending-${t.id}`}>
              Sending…
            </span>
          )}
        </div>
        <p className="tw-text">{t.text}</p>
        <div className="tw-actions">
          <button className="tw-action" data-testid={`replies-${t.id}`} onClick={() => on_open(t)}>
            💬 {reply_count}
          </button>
          <button
            className="tw-action"
            data-liked={eff.liked}
            data-testid={`like-${t.id}`}
            onClick={() => on_like(t)}
          >
            {eff.liked ? '♥' : '♡'} {eff.likes}
          </button>
          <button
            className="tw-action"
            data-retweeted={eff.retweeted}
            data-testid={`rt-${t.id}`}
            onClick={() => on_rt(t)}
          >
            ⇄ {eff.retweets}
          </button>
        </div>
      </div>
    </article>
  )
}

function ComposeBox({ on_post }: { on_post: (text: string) => void }) {
  const [text, set_text] = useState('')
  const remaining = 280 - text.length
  const disabled = text.trim().length === 0 || remaining < 0
  return (
    <div className="tw-compose" data-testid="compose">
      <textarea
        className="tw-compose-input"
        data-testid="compose-input"
        placeholder="What is happening?!"
        value={text}
        onChange={(e) => set_text(e.target.value)}
      />
      <div className="tw-compose-row">
        <span
          className={remaining < 0 ? 'tw-count tw-count-over' : 'tw-count'}
          data-testid="char-count"
        >
          {remaining}
        </span>
        <button
          className="tw-post-btn"
          data-testid="compose-post"
          disabled={disabled}
          onClick={() => {
            on_post(text)
            set_text('')
          }}
        >
          Post
        </button>
      </div>
    </div>
  )
}

export function TwitterApp({
  tweets,
  now_ts,
  me,
}: {
  tweets: Tweet[]
  now_ts: number
  me: TwitterUser
}) {
  const [extra, set_extra] = useState<Tweet[]>([])
  const [overrides, set_overrides] = useState<Record<string, Partial<Effective>>>({})
  const [count, set_count] = useState(BATCH)
  const [view, set_view] = useState<string | null>(null)
  const [toast, set_toast] = useState<string | null>(null)
  const sentinel = useRef<HTMLDivElement | null>(null)
  const next_id = useRef(1)

  const all_tweets = [...extra, ...tweets]
  const feed = all_tweets.filter((t) => !t.replyTo)
  const visible = feed.slice(0, count)
  const has_more = count < feed.length

  useEffect(() => {
    if (!window.IntersectionObserver) return undefined
    const on_intersect = (entries: IntersectionObserverEntry[]) => {
      if (entries[0].isIntersecting) {
        set_count((c) => c + BATCH)
      }
    }
    const obs = new IntersectionObserver(on_intersect, { rootMargin: '200px' })
    if (sentinel.current) obs.observe(sentinel.current)
    return () => obs.disconnect()
  }, [count, view])

  function show_toast(msg: string) {
    set_toast(msg)
    setTimeout(() => set_toast(null), TOAST_MS)
  }

  function effective(t: Tweet): Effective {
    const o = overrides[t.id] ?? {}
    return {
      liked: o.liked ?? false,
      retweeted: o.retweeted ?? false,
      likes: o.likes ?? t.likes,
      retweets: o.retweets ?? t.retweets,
    }
  }

  function set_override(tid: string, patch: Partial<Effective>) {
    set_overrides((o) => ({ ...o, [tid]: { ...(o[tid] ?? {}), ...patch } }))
  }

  function count_replies(tid: string): number {
    return all_tweets.filter((x) => x.replyTo === tid).length
  }

  function post_tweet(text: string) {
    const tid = `local-${next_id.current}`
    next_id.current += 1
    const t: Tweet = {
      id: tid,
      author: me,
      text,
      ts: now_ts,
      likes: 0,
      retweets: 0,
      replyTo: null,
      pending: true,
    }
    set_extra((xs) => [t, ...xs])
    const send = async () => {
      try {
        await send_tweet(text)
        set_extra((xs) => xs.map((x) => (x.id === tid ? { ...x, pending: false } : x)))
      } catch {
        set_extra((xs) => xs.filter((x) => x.id !== tid))
        show_toast('Post failed — rolled back')
      }
    }
    send()
  }

  function toggle_like(t: Tweet) {
    const eff = effective(t)
    const liked = !eff.liked
    const delta = liked ? 1 : -1
    set_override(t.id, { liked, likes: eff.likes + delta })
    if (liked && t.text.includes('#fail')) {
      setTimeout(() => {
        set_override(t.id, { liked: eff.liked, likes: eff.likes })
        show_toast('Like failed — rolled back')
      }, SEND_MS)
    }
  }

  function toggle_rt(t: Tweet) {
    const eff = effective(t)
    const retweeted = !eff.retweeted
    const delta = retweeted ? 1 : -1
    set_override(t.id, { retweeted, retweets: eff.retweets + delta })
    if (retweeted && t.text.includes('#fail')) {
      setTimeout(() => {
        set_override(t.id, { retweeted: eff.retweeted, retweets: eff.retweets })
        show_toast('Retweet failed — rolled back')
      }, SEND_MS)
    }
  }

  function open_thread(t: Tweet) {
    set_view(t.id)
  }

  let body
  if (view === null) {
    body = (
      <main className="tw-feed" data-testid="feed">
        <ComposeBox on_post={post_tweet} />
        {visible.map((t) => (
          <TweetCard
            key={t.id}
            t={t}
            eff={effective(t)}
            reply_count={count_replies(t.id)}
            now_ts={now_ts}
            on_open={open_thread}
            on_like={toggle_like}
            on_rt={toggle_rt}
          />
        ))}
        {has_more && <div className="tw-sentinel" data-testid="feed-sentinel" ref={sentinel} />}
        <div className="tw-feed-status" data-testid="feed-count">
          {visible.length} of {feed.length}
        </div>
      </main>
    )
  } else {
    const parent_matches = all_tweets.filter((x) => x.id === view)
    const parent = parent_matches.length > 0 ? parent_matches[0] : null
    const replies = all_tweets.filter((x) => x.replyTo === view)
    body = (
      <section className="tw-thread" data-testid="thread">
        <button className="tw-back" data-testid="thread-back" onClick={() => set_view(null)}>
          ← Back
        </button>
        {parent && (
          <TweetCard
            t={parent}
            eff={effective(parent)}
            reply_count={replies.length}
            now_ts={now_ts}
            on_open={open_thread}
            on_like={toggle_like}
            on_rt={toggle_rt}
          />
        )}
        <h2 className="tw-thread-title">Replies ({replies.length})</h2>
        {replies.map((t) => (
          <TweetCard
            key={t.id}
            t={t}
            eff={effective(t)}
            reply_count={count_replies(t.id)}
            now_ts={now_ts}
            on_open={open_thread}
            on_like={toggle_like}
            on_rt={toggle_rt}
          />
        ))}
        {replies.length === 0 && <p className="tw-no-replies">No replies yet.</p>}
      </section>
    )
  }

  return (
    <div className="tw-app" data-testid="twitter-app">
      <header className="tw-header">
        <h1>Chirper</h1>
      </header>
      {toast && (
        <div className="tw-toast" data-testid="toast" role="status">
          {toast}
        </div>
      )}
      {body}
    </div>
  )
}

export default TwitterApp
