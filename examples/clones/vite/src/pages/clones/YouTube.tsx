import { Link } from 'react-router'
import { YouTubeApp } from '../../../../shared/youtube/YouTubeApp.ps'
import { youtube_videos } from '../../../../shared/youtube/fixtures'

// YouTube clone — production track. Mounts the shared tri-track island
// (loaded LIVE from .ps by vite-plugin-pyths) + fixture pass-through only;
// all clone logic lives in shared/youtube/ (see CONTRIBUTING.md).
export default function YouTube() {
  return (
    <main className="yt-page">
      <p className="yt-home-link">
        <Link to="/">&larr; home</Link>
      </p>
      <YouTubeApp videos={youtube_videos} />
    </main>
  )
}
