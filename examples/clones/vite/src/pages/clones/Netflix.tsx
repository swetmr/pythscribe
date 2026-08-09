import { NetflixApp } from '../../../../shared/netflix/NetflixApp.ps'
import { netflixFixture } from '../../../../shared/netflix/fixtures'

// Netflix clone — production track. Mounts the shared tri-track PythScribe
// island (loaded LIVE by vite-plugin-pyths, no precompile step) with the
// local offline fixtures. Full-bleed layout: the clone brings its own
// chrome (.nf-app), so no .shell wrapper here.
export default function Netflix() {
  return <NetflixApp {...netflixFixture} />
}
