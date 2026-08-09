'use client'
import { createContext, useContext, useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { createPortal } from 'react-dom'
import './Netflix.css'
import type { NetflixRow, NetflixTitle } from './fixtures'

/**
 * Netflix clone — React reference oracle. Dual-track with NetflixApp.ps /
 * NetflixApp.psc; all three must render identical DOM for the same props
 * (see the *.test.tsx parity suites in this directory).
 *
 * Deliberately a SINGLE module: in the Next shell, "use client" .ps islands
 * are precompiled to sibling .client.js files whose extensionless relative
 * imports are NOT loader-rewritten, so a cross-file .ps -> .ps import inside
 * the client graph would resolve to the .tsx oracle instead (resolveExtensions
 * puts .tsx first). One island file per clone sidesteps that entirely; the
 * oracle mirrors the same single-file layout so the tracks stay comparable.
 *
 * Prop names are snake_case in ALL tracks (PythScribe passes user-component
 * props through unchanged), so the parity tests can spread the same props
 * into every track.
 */

export interface MyListValue {
  ids: string[]
  toggle: (tid: string) => void
}

export const MyListContext = createContext<MyListValue | null>(null)

export function use_my_list(): MyListValue {
  return useContext(MyListContext) as MyListValue
}

export function MyListProvider({ children = null }: { children?: ReactNode }) {
  const [ids, set_ids] = useState<string[]>([])
  function toggle(tid: string) {
    if (ids.includes(tid)) {
      set_ids(ids.filter((i) => i !== tid))
    } else {
      set_ids([...ids, tid])
    }
  }
  const value = { ids: ids, toggle: toggle }
  return <MyListContext.Provider value={value}>{children}</MyListContext.Provider>
}

export function Hero({
  title,
  on_more_info,
}: {
  title: NetflixTitle
  on_more_info: (t: NetflixTitle) => void
}) {
  return (
    <header
      className="nf-hero"
      data-testid="nf-hero"
      style={{
        backgroundImage: title.poster
          ? `linear-gradient(90deg, rgba(20,20,20,0.9) 0%, rgba(20,20,20,0.45) 55%, rgba(20,20,20,0.1) 100%), url(${title.poster})`
          : `linear-gradient(160deg, ${title.art[0]} 0%, ${title.art[1]} 100%)`,
        backgroundColor: title.art[0],
        backgroundSize: 'cover',
        backgroundPosition: 'center',
      }}
    >
      <div className="nf-hero-content">
        <p className="nf-hero-kicker">N SERIES</p>
        <h1 className="nf-hero-title" data-testid="nf-hero-title">
          {title.name}
        </h1>
        <p className="nf-meta">
          <span className="nf-match">{`${title.match}% Match`}</span>
          <span>{title.year}</span>
          <span className="nf-maturity">{title.maturity}</span>
          <span>{title.duration}</span>
        </p>
        <p className="nf-hero-desc" data-testid="nf-hero-desc">
          {title.description}
        </p>
        <div className="nf-hero-ctas">
          <button className="nf-btn nf-btn-play" data-testid="nf-hero-play">
            &#9654; Play
          </button>
          <button
            className="nf-btn nf-btn-info"
            data-testid="nf-hero-info"
            onClick={() => on_more_info(title)}
          >
            More Info
          </button>
        </div>
      </div>
    </header>
  )
}

export function TitleCard({
  row_id,
  title,
  on_open,
}: {
  row_id: string
  title: NetflixTitle
  on_open: (t: NetflixTitle) => void
}) {
  const ctx = use_my_list()
  const tid = title.id
  const in_list = ctx.ids.includes(tid)
  function handle_toggle(e: { stopPropagation: () => void }) {
    e.stopPropagation()
    ctx.toggle(tid)
  }
  return (
    <div
      className="nf-card"
      data-testid={`nf-card-${row_id}-${tid}`}
      data-in-list={in_list}
      onClick={() => on_open(title)}
    >
      <div
        className="nf-card-art"
        style={{
          backgroundImage: title.poster
            ? `url(${title.poster})`
            : `linear-gradient(135deg, ${title.art[0]}, ${title.art[1]})`,
          backgroundColor: title.art[0],
          backgroundSize: 'cover',
          backgroundPosition: 'center',
        }}
      >
        <span className="nf-card-name">{title.name}</span>
      </div>
      <div className="nf-card-preview" data-testid={`nf-preview-${row_id}-${tid}`}>
        <span className="nf-match">{`${title.match}% Match`}</span>
        <span className="nf-card-meta">{`${title.year} | ${title.duration}`}</span>
        <button
          className="nf-card-toggle"
          data-testid={`nf-toggle-${row_id}-${tid}`}
          aria-label={
            in_list ? `Remove ${title.name} from My List` : `Add ${title.name} to My List`
          }
          onClick={handle_toggle}
        >
          {in_list ? '✓' : '+'}
        </button>
      </div>
    </div>
  )
}

export function Carousel({
  row_id,
  label,
  titles,
  on_open,
}: {
  row_id: string
  label: string
  titles: NetflixTitle[]
  on_open: (t: NetflixTitle) => void
}) {
  const track_ref = useRef<HTMLDivElement | null>(null)
  function scroll(direction: number) {
    const el = track_ref.current
    if (el) {
      el.scrollBy({ left: direction * 600, behavior: 'smooth' })
    }
  }
  return (
    <section className="nf-row" data-testid={`nf-row-${row_id}`}>
      <h2 className="nf-row-label">{label}</h2>
      <div className="nf-row-body">
        <button
          className="nf-scroll nf-scroll-left"
          data-testid={`nf-scroll-left-${row_id}`}
          aria-label={`Scroll ${label} left`}
          onClick={() => scroll(-1)}
        >
          {'‹'}
        </button>
        <div className="nf-track" data-testid={`nf-track-${row_id}`} ref={track_ref}>
          {titles.map((t) => (
            <TitleCard key={t.id} row_id={row_id} title={t} on_open={on_open} />
          ))}
        </div>
        <button
          className="nf-scroll nf-scroll-right"
          data-testid={`nf-scroll-right-${row_id}`}
          aria-label={`Scroll ${label} right`}
          onClick={() => scroll(1)}
        >
          {'›'}
        </button>
      </div>
    </section>
  )
}

export function DetailModal({
  title,
  on_close,
}: {
  title: NetflixTitle
  on_close: () => void
}) {
  const ctx = use_my_list()
  const tid = title.id
  const in_list = ctx.ids.includes(tid)
  useEffect(() => {
    function on_key(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        on_close()
      }
    }
    document.addEventListener('keydown', on_key)
    const prev = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', on_key)
      document.body.style.overflow = prev
    }
  }, [on_close])
  return createPortal(
    <div className="nf-backdrop" data-testid="nf-modal-backdrop" onClick={on_close}>
      <div
        className="nf-modal"
        data-testid="nf-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          className="nf-modal-close"
          data-testid="nf-modal-close"
          aria-label="Close"
          onClick={on_close}
        >
          {'✕'}
        </button>
        <div
          className="nf-modal-art"
          style={{
            backgroundImage: title.poster
              ? `url(${title.poster})`
              : `linear-gradient(135deg, ${title.art[0]}, ${title.art[1]})`,
            backgroundColor: title.art[0],
            backgroundSize: 'cover',
            backgroundPosition: 'center',
          }}
        >
          <span className="nf-card-name">{title.name}</span>
        </div>
        <div className="nf-modal-body">
          <video
            className="nf-modal-preview"
            data-testid="nf-modal-preview"
            src="/media/sample.webm"
            muted
            autoPlay
            loop
            playsInline
          />
          <h2 data-testid="nf-modal-title">{title.name}</h2>
          <p className="nf-meta">
            <span className="nf-match">{`${title.match}% Match`}</span>
            <span>{title.year}</span>
            <span className="nf-maturity">{title.maturity}</span>
            <span>{title.duration}</span>
          </p>
          <p className="nf-modal-desc" data-testid="nf-modal-desc">
            {title.description}
          </p>
          <p className="nf-modal-genres">{title.genres.join(' | ')}</p>
          <button
            className="nf-btn nf-btn-info"
            data-testid="nf-modal-toggle"
            onClick={() => ctx.toggle(tid)}
          >
            {in_list ? '✓ In My List' : '+ Add to My List'}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}

export function NetflixBrowse({
  titles,
  rows,
  hero_id,
}: {
  titles: Record<string, NetflixTitle>
  rows: NetflixRow[]
  hero_id: string
}) {
  const ctx = use_my_list()
  const [sel, set_sel] = useState<NetflixTitle | null>(null)
  function close_modal() {
    set_sel(null)
  }
  const my_titles = ctx.ids.map((tid) => titles[tid])
  return (
    <div className="nf-app" data-testid="nf-app">
      <Hero title={titles[hero_id]} on_more_info={set_sel} />
      <main className="nf-rows">
        <section className="nf-mylist" data-testid="nf-mylist-section">
          {my_titles.length > 0 ? (
            <Carousel row_id="my-list" label="My List" titles={my_titles} on_open={set_sel} />
          ) : (
            <div className="nf-row">
              <h2 className="nf-row-label">My List</h2>
              <p className="nf-mylist-empty" data-testid="nf-mylist-empty">
                Your list is empty. Add titles with the + button.
              </p>
            </div>
          )}
        </section>
        {rows.map((r) => (
          <Carousel
            key={r.id}
            row_id={r.id}
            label={r.label}
            titles={r.title_ids.map((tid) => titles[tid])}
            on_open={set_sel}
          />
        ))}
      </main>
      {sel && <DetailModal title={sel} on_close={close_modal} />}
    </div>
  )
}

export function NetflixApp({
  titles,
  rows,
  hero_id,
}: {
  titles: Record<string, NetflixTitle>
  rows: NetflixRow[]
  hero_id: string
}) {
  return (
    <MyListProvider>
      <NetflixBrowse titles={titles} rows={rows} hero_id={hero_id} />
    </MyListProvider>
  )
}

export default NetflixApp
