/**
 * Netflix clone — React reference oracle mirror at /react-reference/netflix.
 * Renders the NetflixApp.tsx "use client" island directly (no precompile
 * needed — Next compiles plain .tsx through its normal pipeline).
 * Dual-track with app/netflix/page.ps.
 */
import { NetflixApp } from '../../../../shared/netflix/NetflixApp'
import { netflixFixture } from '../../../../shared/netflix/fixtures'

export default function Page() {
  return <NetflixApp {...netflixFixture} />
}
