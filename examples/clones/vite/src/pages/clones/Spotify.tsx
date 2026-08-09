import { Link } from 'react-router'
import { SpotifyApp } from '../../../../shared/spotify/SpotifyApp.ps'
import { PLAYLISTS } from '../../../../shared/spotify/fixtures'

// Demo-page ONLY real-music injection: the shared fixture keeps pointing every
// track at the ~1s /media/sample.wav so the render-parity suite + e2e (which
// rely on the short-clip auto-advance timing) stay untouched. Here we swap in
// the 10 royalty-free tracks, cycled deterministically across every track so
// /spotify plays real, varied music. (See vite/public/media/CREDITS.md.)
let musicIdx = 0
const REAL_PLAYLISTS = PLAYLISTS.map((pl) => ({
  ...pl,
  tracks: pl.tracks.map((t) => {
    const src = `/media/music/track${musicIdx % 10}.mp3`
    musicIdx += 1
    return { ...t, src }
  }),
}))

// Spotify clone — production track. Mounts the shared .ps island, loaded
// LIVE by vite-plugin-pyths (no precompile step). Do NOT edit App.tsx
// (routes are pre-wired).
export default function Spotify() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <SpotifyApp playlists={REAL_PLAYLISTS} />
    </div>
  )
}
