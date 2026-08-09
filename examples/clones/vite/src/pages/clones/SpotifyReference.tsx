import { Link } from 'react-router'
import { SpotifyApp } from '../../../../shared/spotify/SpotifyApp'
import { PLAYLISTS } from '../../../../shared/spotify/fixtures'

// Spotify clone — React reference oracle at /react-reference/spotify,
// mirroring the /spotify production route. Do NOT edit App.tsx.
export default function SpotifyReference() {
  return (
    <div className="shell">
      <p>
        <Link to="/">&larr; home</Link>
      </p>
      <SpotifyApp playlists={PLAYLISTS} />
    </div>
  )
}
