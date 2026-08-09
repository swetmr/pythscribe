import { NetflixApp } from '../../../../shared/netflix/NetflixApp.psc'
import { netflixFixture } from '../../../../shared/netflix/fixtures'

// Netflix clone — production track. Mounts the shared tri-track PythScribe
// island (loaded LIVE by vite-plugin-pyths, no precompile step) with the
// local offline fixtures. Full-bleed layout: the clone brings its own
// chrome (.nf-app), so no .shell wrapper here.
export default function NetflixPsc() {
  return <NetflixApp {...netflixFixture} />
}
