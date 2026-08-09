'use client'
import { useEffect, useReducer, useRef, useState } from 'react'
import './SpotifyApp.css'
import { seededShuffle, SHUFFLE_SEED, type Playlist, type Track } from './fixtures'

/**
 * React reference oracle for the Spotify clone.
 * Dual-track-paired with SpotifyApp.ps / SpotifyApp.psc — all three must
 * render identical DOM for the same props (see SpotifyApp.test.tsx).
 *
 * Feature surface (the PythScribe stress test):
 *  - sidebar playlist nav + "Up next" queue panel
 *  - track table (number / title / artist / duration, hover play, row menu
 *    with add-to-queue, active-row highlight)
 *  - persistent player bar driving a REAL <audio> element (/media/sample.wav):
 *    play/pause, next/prev over the queue, seek bound to timeupdate, volume,
 *    elapsed/total time, auto-advance on `ended`
 *  - deterministic seeded shuffle (Park-Miller LCG, fixed SHUFFLE_SEED)
 *  - Space toggles playback unless focus is in a form field
 *  - playback state machine in useReducer
 */

export interface PlayerState {
  playlistId: string
  queue: Track[]
  index: number
  status: 'idle' | 'playing' | 'paused'
  shuffle: boolean
  volume: number
}

export type PlayerAction =
  | { type: 'SELECT_PLAYLIST'; id: string }
  | { type: 'PLAY_TRACK'; tracks: Track[]; trackId: string }
  | { type: 'TOGGLE_PLAY' }
  | { type: 'NEXT' }
  | { type: 'PREV' }
  | { type: 'ENDED' }
  | { type: 'ADD_TO_QUEUE'; track: Track }
  | { type: 'TOGGLE_SHUFFLE' }
  | { type: 'SET_VOLUME'; value: number }

export function reducer(state: PlayerState, action: PlayerAction): PlayerState {
  if (action.type === 'SELECT_PLAYLIST') {
    return { ...state, playlistId: action.id }
  }
  if (action.type === 'PLAY_TRACK') {
    const tracks = action.tracks
    let pos = -1
    for (let i = 0; i < tracks.length; i += 1) {
      if (tracks[i].id === action.trackId) pos = i
    }
    if (pos < 0) return state
    if (state.shuffle) {
      const rest = tracks.filter((t) => t.id !== action.trackId)
      const queue = [tracks[pos], ...seededShuffle(rest, SHUFFLE_SEED)]
      return { ...state, queue, index: 0, status: 'playing' }
    }
    return { ...state, queue: tracks.slice(), index: pos, status: 'playing' }
  }
  if (action.type === 'TOGGLE_PLAY') {
    if (state.status === 'playing') return { ...state, status: 'paused' }
    if (state.queue.length === 0) return state
    if (state.index < 0) return { ...state, index: 0, status: 'playing' }
    return { ...state, status: 'playing' }
  }
  if (action.type === 'NEXT') {
    if (state.index + 1 < state.queue.length) {
      return { ...state, index: state.index + 1, status: 'playing' }
    }
    return { ...state, status: 'paused' }
  }
  if (action.type === 'PREV') {
    if (state.index > 0) return { ...state, index: state.index - 1, status: 'playing' }
    return state
  }
  if (action.type === 'ENDED') {
    if (state.index + 1 < state.queue.length) {
      return { ...state, index: state.index + 1, status: 'playing' }
    }
    return { ...state, status: 'idle' }
  }
  if (action.type === 'ADD_TO_QUEUE') {
    return { ...state, queue: [...state.queue, action.track] }
  }
  if (action.type === 'TOGGLE_SHUFFLE') {
    const on = !state.shuffle
    if (!on || state.queue.length === 0) return { ...state, shuffle: on }
    const head = state.queue.slice(0, state.index + 1)
    const rest = seededShuffle(state.queue.slice(state.index + 1), SHUFFLE_SEED)
    return { ...state, shuffle: on, queue: [...head, ...rest] }
  }
  if (action.type === 'SET_VOLUME') {
    return { ...state, volume: action.value }
  }
  return state
}

export function fmtTime(sec: number): string {
  const total = Math.trunc(sec)
  const m = Math.trunc(total / 60)
  const s = total % 60
  return String(m) + ':' + (s < 10 ? '0' + String(s) : String(s))
}

export interface TrackRowProps {
  track: Track
  num: number
  active: boolean
  menuOpen: boolean
  onPlay: (track: Track) => void
  onToggleMenu: (track: Track) => void
  onAdd: (track: Track) => void
}

export function TrackRow({ track, num, active, menuOpen, onPlay, onToggleMenu, onAdd }: TrackRowProps) {
  return (
    <tr
      className={active ? 'sp-track-row active' : 'sp-track-row'}
      data-testid={'track-row-' + track.id}
      data-active={active ? 'true' : 'false'}
    >
      <td className="col-num">
        <span className="sp-track-num">{num}</span>
        <button
          className="sp-row-play"
          data-testid={'track-play-' + track.id}
          aria-label={'Play ' + track.title}
          onClick={() => onPlay(track)}
        >
          ▶
        </button>
      </td>
      <td className="sp-track-title">{track.title}</td>
      <td className="sp-track-artist">{track.artist}</td>
      <td className="col-dur">{fmtTime(track.duration)}</td>
      <td className="col-menu">
        <button
          className="sp-row-menu-btn"
          data-testid={'track-menu-' + track.id}
          aria-label={'More options for ' + track.title}
          onClick={() => onToggleMenu(track)}
        >
          ⋯
        </button>
        {menuOpen && (
          <div className="sp-row-menu" data-testid={'row-menu-' + track.id}>
            <button data-testid={'queue-add-' + track.id} onClick={() => onAdd(track)}>
              Add to queue
            </button>
          </div>
        )}
      </td>
    </tr>
  )
}

export interface SpotifyAppProps {
  playlists: Playlist[]
}

export function SpotifyApp({ playlists }: SpotifyAppProps) {
  const [state, dispatch] = useReducer(reducer, {
    playlistId: playlists[0].id,
    queue: [],
    index: -1,
    status: 'idle',
    shuffle: false,
    volume: 0.8,
  })
  const audioRef = useRef<HTMLAudioElement>(null)
  const [currentTime, setCurrentTime] = useState(0)
  const [duration, setDuration] = useState(0)
  const [menuId, setMenuId] = useState<string | null>(null)

  const matches = playlists.filter((pl) => pl.id === state.playlistId)
  const playlist = matches.length > 0 ? matches[0] : playlists[0]
  const current = state.index >= 0 ? state.queue[state.index] : null
  const currentId = current ? current.id : null
  const upNext = state.queue.slice(state.index + 1)

  // Restart from 0 whenever the active track changes (every fixture track
  // shares the same offline asset, so the src attribute never changes).
  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    try {
      audio.currentTime = 0
    } catch {
      // jsdom: media stubs may reject; ignore
    }
    setCurrentTime(0)
  }, [currentId])

  // Drive the real <audio> element from the state machine.
  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    if (state.status === 'playing' && currentId) {
      let p
      try {
        p = audio.play()
      } catch {
        p = undefined
      }
      if (p && typeof p.catch === 'function') p.catch(() => undefined)
    } else {
      try {
        audio.pause()
      } catch {
        // jsdom: not implemented; ignore
      }
    }
  }, [state.status, currentId])

  useEffect(() => {
    const audio = audioRef.current
    if (audio) audio.volume = state.volume
  }, [state.volume])

  // Space toggles playback — unless focus is in a form field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code !== 'Space') return
      const el = e.target as HTMLElement | null
      const tag = el && el.tagName ? el.tagName : ''
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (el && el.isContentEditable) return
      e.preventDefault()
      dispatch({ type: 'TOGGLE_PLAY' })
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const playTrack = (track: Track) => {
    dispatch({ type: 'PLAY_TRACK', tracks: playlist.tracks, trackId: track.id })
  }
  const toggleMenu = (track: Track) => {
    setMenuId(menuId === track.id ? null : track.id)
  }
  const addToQueue = (track: Track) => {
    dispatch({ type: 'ADD_TO_QUEUE', track })
    setMenuId(null)
  }
  const onSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = parseFloat(e.target.value)
    const audio = audioRef.current
    if (audio) {
      try {
        audio.currentTime = v
      } catch {
        // jsdom: ignore
      }
    }
    setCurrentTime(v)
  }
  const onTimeUpdate = () => {
    const audio = audioRef.current
    if (audio) setCurrentTime(audio.currentTime)
  }
  const onLoadedMetadata = () => {
    const audio = audioRef.current
    if (audio) setDuration(audio.duration)
  }

  return (
    <div className="sp-app" data-testid="spotify-app">
      <div className="sp-body">
        <aside className="sp-sidebar" data-testid="sp-sidebar">
          <h2 className="sp-logo">Spotify</h2>
          <ul className="sp-playlists" data-testid="sp-playlists">
            {playlists.map((pl) => (
              <li key={pl.id}>
                <button
                  className={pl.id === state.playlistId ? 'sp-playlist-btn active' : 'sp-playlist-btn'}
                  data-testid={'playlist-' + pl.id}
                  data-active={pl.id === state.playlistId ? 'true' : 'false'}
                  onClick={() => dispatch({ type: 'SELECT_PLAYLIST', id: pl.id })}
                >
                  {pl.name}
                </button>
              </li>
            ))}
          </ul>
          <div className="sp-queue" data-testid="sp-queue">
            <h3>Up next</h3>
            {upNext.length === 0 ? (
              <p className="sp-queue-empty" data-testid="queue-empty">
                Queue is empty
              </p>
            ) : (
              <ol className="sp-queue-list" data-testid="queue-list">
                {upNext.map((t, i) => (
                  <li key={i} className="sp-queue-item" data-testid="queue-item">
                    {t.title}
                  </li>
                ))}
              </ol>
            )}
          </div>
        </aside>
        <main className="sp-main">
          <h1 data-testid="sp-playlist-title">{playlist.name}</h1>
          <p className="sp-playlist-meta" data-testid="sp-playlist-meta">
            {String(playlist.tracks.length) + ' tracks'}
          </p>
          <table className="sp-tracks" data-testid="sp-track-table">
            <thead>
              <tr>
                <th className="col-num">#</th>
                <th>Title</th>
                <th>Artist</th>
                <th className="col-dur">Time</th>
                <th className="col-menu"></th>
              </tr>
            </thead>
            <tbody>
              {playlist.tracks.map((t, i) => (
                <TrackRow
                  key={t.id}
                  track={t}
                  num={i + 1}
                  active={current ? current.id === t.id : false}
                  menuOpen={menuId === t.id}
                  onPlay={playTrack}
                  onToggleMenu={toggleMenu}
                  onAdd={addToQueue}
                />
              ))}
            </tbody>
          </table>
        </main>
      </div>
      <footer className="sp-player" data-testid="sp-player">
        <div className="sp-now">
          <span className="sp-now-title" data-testid="player-track-title">
            {current ? current.title : 'Nothing playing'}
          </span>
          <span className="sp-now-artist" data-testid="player-track-artist">
            {current ? current.artist : ''}
          </span>
        </div>
        <div className="sp-controls">
          <button
            className={state.shuffle ? 'sp-ctl sp-shuffle on' : 'sp-ctl sp-shuffle'}
            data-testid="player-shuffle"
            aria-pressed={state.shuffle ? 'true' : 'false'}
            aria-label="Toggle shuffle"
            onClick={() => dispatch({ type: 'TOGGLE_SHUFFLE' })}
          >
            🔀
          </button>
          <button
            className="sp-ctl"
            data-testid="player-prev"
            aria-label="Previous track"
            onClick={() => dispatch({ type: 'PREV' })}
          >
            ⏮
          </button>
          <button
            className="sp-ctl sp-play-btn"
            data-testid="player-play"
            aria-label={state.status === 'playing' ? 'Pause' : 'Play'}
            onClick={() => dispatch({ type: 'TOGGLE_PLAY' })}
          >
            {state.status === 'playing' ? '⏸' : '▶'}
          </button>
          <button
            className="sp-ctl"
            data-testid="player-next"
            aria-label="Next track"
            onClick={() => dispatch({ type: 'NEXT' })}
          >
            ⏭
          </button>
        </div>
        <div className="sp-timeline">
          <span className="sp-time" data-testid="player-elapsed">
            {fmtTime(currentTime)}
          </span>
          <input
            type="range"
            className="sp-seek"
            data-testid="player-seek"
            aria-label="Seek"
            min={0}
            max={duration > 0 ? duration : 0}
            step={0.01}
            value={currentTime < duration ? currentTime : duration > 0 ? duration : 0}
            onChange={onSeek}
          />
          <span className="sp-time" data-testid="player-total">
            {fmtTime(duration > 0 ? duration : 0)}
          </span>
        </div>
        <div className="sp-volume">
          <input
            type="range"
            className="sp-vol"
            data-testid="player-volume"
            aria-label="Volume"
            min={0}
            max={1}
            step={0.01}
            value={state.volume}
            onChange={(e) => dispatch({ type: 'SET_VOLUME', value: parseFloat(e.target.value) })}
          />
        </div>
        <audio
          ref={audioRef}
          data-testid="player-audio"
          src={current ? current.src : undefined}
          preload="auto"
          onTimeUpdate={onTimeUpdate}
          onLoadedMetadata={onLoadedMetadata}
          onEnded={() => dispatch({ type: 'ENDED' })}
        />
      </footer>
    </div>
  )
}

export default SpotifyApp
