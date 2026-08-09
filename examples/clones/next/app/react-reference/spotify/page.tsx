/**
 * Spotify clone oracle page (React reference track).
 * Renders the SpotifyApp.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/spotify/page.ps.
 */
import { SpotifyApp } from '../../../../shared/spotify/SpotifyApp'
import { PLAYLISTS } from '../../../../shared/spotify/fixtures'

export default function Page() {
  return (
    <main className="shell">
      <p>
        <a href="/">&larr; home</a>
      </p>
      <SpotifyApp playlists={PLAYLISTS} />
    </main>
  )
}
