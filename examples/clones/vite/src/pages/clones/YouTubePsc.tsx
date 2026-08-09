import { Link } from 'react-router'
import { YouTubeApp } from '../../../../shared/youtube/YouTubeApp.psc'
import { youtube_videos } from '../../../../shared/youtube/fixtures'

// YouTube clone — .psc (compressed) track. Same shared island, loaded from
// the COMPRESSED YouTubeApp.psc (vite-plugin-pyths expands + compiles live).
// Renders identically to the .ps track by the Iron Rule.
export default function YouTubePsc() {
  return (
    <main className="yt-page">
      <p className="yt-home-link">
        <Link to="/">&larr; home</Link> &nbsp;·&nbsp; <strong>.psc (compressed) track</strong>
      </p>
      <YouTubeApp videos={youtube_videos} />
    </main>
  )
}
