import { Link } from 'react-router'
import { YouTubeApp } from '../../../../shared/youtube/YouTubeApp'
import { youtube_videos } from '../../../../shared/youtube/fixtures'

// YouTube clone — React reference oracle track, mirroring /youtube at
// /react-reference/youtube. Extensionless import resolves the .tsx oracle
// (Vite's resolve.extensions does not include .ps).
export default function YouTubeReference() {
  return (
    <main className="yt-page">
      <p className="yt-home-link">
        <Link to="/">&larr; home</Link>
      </p>
      <YouTubeApp videos={youtube_videos} />
    </main>
  )
}
