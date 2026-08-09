// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). 48 deterministic generated videos; snake_case export
// name so the Next production page (a .ps server page) can import it without
// any name mapping.

export interface YtVideo {
  id: string
  title: string
  channel: string
  views: string
  age: string
  duration: string
  color: string
  thumb: string
}

const CHANNELS = [
  'PixelForge',
  'Trailhead Labs',
  'Synthwave Cinema',
  'Daily Orbit',
  'Kernel Panic',
  'The Slow Cooker',
  'Metric Space',
  'Cobalt Garage',
]

const TOPICS = [
  'Building a Compiler in Rust — Full Course',
  'React Server Components Explained',
  'The Physics of Black Holes',
  'Lo-fi Beats to Debug To',
  'Homemade Ramen From Scratch',
  'Speedrunning Zelda: A History',
  'Machine Learning on a Potato PC',
  'Van Life: 30 Days Across Iceland',
  'The Art of CSS Grid',
  'Why Planes Fly: Aerodynamics 101',
  'Restoring a 1970s Synthesizer',
  'Chess Openings Tier List',
]

const DURATIONS = ['4:07', '12:34', '0:58', '23:11', '8:45', '15:02', '1:30', '9:59', '31:44', '6:18', '2:47', '17:25']
const VIEWS = ['1.2M', '845K', '12K', '3.4M', '98K', '412K', '7.1M', '56K', '230K', '1.8M', '9.4K', '670K']
const AGES = [
  '2 hours ago',
  '1 day ago',
  '3 days ago',
  '1 week ago',
  '2 weeks ago',
  '1 month ago',
  '3 months ago',
  '6 months ago',
  '1 year ago',
  '2 years ago',
  '3 years ago',
  '5 years ago',
]

export const youtube_videos: YtVideo[] = Array.from({ length: 48 }, (_, i) => ({
  id: `v${String(i + 1).padStart(2, '0')}`,
  title: i < 12 ? TOPICS[i] : `${TOPICS[i % 12]} — Part ${Math.floor(i / 12) + 1}`,
  channel: CHANNELS[i % 8],
  views: VIEWS[(i * 5) % 12],
  age: AGES[(i * 7) % 12],
  duration: DURATIONS[(i * 11) % 12],
  color: `hsl(${(i * 37) % 360} 45% 30%)`,
  thumb: `/media/thumbs/thumb${i % 12}.jpg`,
}))
