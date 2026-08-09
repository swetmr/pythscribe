// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). 4 playlists x 8 tracks; every track points at the
// canonical offline media asset /media/sample.wav (~1s PCM tone) so real
// <audio> playback — including the `ended` auto-advance path — is testable
// fully offline in both app shells.

export interface Track {
  id: string
  title: string
  artist: string
  duration: number // seconds (display metadata; real asset is ~1s)
  src: string
}

export interface Playlist {
  id: string
  name: string
  tracks: Track[]
}

export const AUDIO_SRC = '/media/sample.wav'

// Fixed seed for the deterministic shuffle — tests (vitest + Playwright)
// assert the EXACT post-shuffle queue order using this seed.
export const SHUFFLE_SEED = 1337

/**
 * Deterministic seeded shuffle: Fisher-Yates driven by a Park-Miller LCG
 * (s = s * 48271 % 2147483647 — stays inside Number-safe integer range).
 * Re-implemented verbatim inside SpotifyApp.ps/.psc (PythScribe cannot yet
 * import from .ts modules); the render-parity suite + e2e specs import THIS
 * copy to compute expected orders, so any cross-language drift fails tests.
 */
export function seededShuffle<T>(items: T[], seed: number): T[] {
  const arr = items.slice()
  let s = seed
  let i = arr.length - 1
  while (i > 0) {
    s = (s * 48271) % 2147483647
    const j = s % (i + 1)
    const tmp = arr[i]
    arr[i] = arr[j]
    arr[j] = tmp
    i -= 1
  }
  return arr
}

function playlist(id: string, name: string, rows: [string, string, number][]): Playlist {
  return {
    id,
    name,
    tracks: rows.map(([title, artist, duration], i) => ({
      id: `${id}-${i + 1}`,
      title,
      artist,
      duration,
      src: AUDIO_SRC,
    })),
  }
}

export const PLAYLISTS: Playlist[] = [
  playlist('focus', 'Deep Focus', [
    ['Midnight Compile', 'The Lexers', 214],
    ['Garbage Collected Heart', 'Heap & Stack', 187],
    ['Tail Call', 'The Recursives', 243],
    ['Idempotent Love', 'Pure Functions', 198],
    ['Race Condition Blues', 'The Mutex Trio', 176],
    ['Off By One', 'Array Index', 205],
    ['Segfault Serenade', 'Null Pointer', 231],
    ['Lazy Evaluation', 'Thunk Machine', 189],
  ]),
  playlist('roadtrip', 'Road Trip Mix', [
    ['Highway Cache Miss', 'The Prefetchers', 224],
    ['Two-Lane Pipeline', 'Sideband', 196],
    ['Cruise Control Flow', 'Branch Predictors', 208],
    ['Gas Station Garbage', 'The Collectors', 182],
    ['Detour Exception', 'Try & Catch', 217],
    ['Mile Marker Zero', 'Init Systems', 201],
    ['Radio Static Types', 'The Checkers', 193],
    ['Rest Stop REST', 'Verb & Noun', 239],
  ]),
  playlist('lofi', 'Lo-Fi Beats', [
    ['Rainy Buffer', 'Chill Threads', 168],
    ['Coffee Shop Daemon', 'Background Process', 172],
    ['Soft Reboot', 'Sleepy Kernel', 184],
    ['Window Seat Manager', 'The Tilers', 177],
    ['Late Night Linter', 'Warning Lights', 191],
    ['Slow Merge', 'Conflict Resolution', 163],
    ['Amber Terminal', 'Phosphor Glow', 179],
    ['Quiet Deploy', 'Friday Never', 186],
  ]),
  playlist('workout', 'Power Workout', [
    ['Maximum Throughput', 'The Benchmarks', 202],
    ['Burn CPU Burn', 'Hot Loop', 178],
    ['Heavy Lifting (ORM Mix)', 'The Migrators', 226],
    ['Sprint Planning', 'Agile Rage', 169],
    ['Overclocked', 'Thermal Throttle', 211],
    ['One More Rep(o)', 'Git Fit', 195],
    ['Cardio Cascade', 'Style Sheets', 188],
    ['Cooldown Period', 'Rate Limiters', 234],
  ]),
]
