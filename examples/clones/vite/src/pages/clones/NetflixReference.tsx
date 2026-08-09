import { NetflixApp } from '../../../../shared/netflix/NetflixApp'
import { netflixFixture } from '../../../../shared/netflix/fixtures'

// Netflix clone — React reference oracle at /react-reference/netflix,
// mirroring the /netflix production route (extensionless import resolves
// the .tsx track).
export default function NetflixReference() {
  return <NetflixApp {...netflixFixture} />
}
