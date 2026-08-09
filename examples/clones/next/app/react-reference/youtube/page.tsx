/**
 * YouTube clone oracle page (React reference track).
 * Renders the YouTubeApp.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/youtube/page.ps.
 */
import { YouTubeApp } from '../../../../shared/youtube/YouTubeApp'
import { youtube_videos } from '../../../../shared/youtube/fixtures'

export default function Page() {
  return (
    <main className="yt-page">
      <p className="yt-home-link">
        <a href="/">&larr; home</a>
      </p>
      <YouTubeApp videos={youtube_videos} />
    </main>
  )
}
