// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md).
// Deterministic Twitter-clone fixtures: a fixed NOW_TS anchor plus tweets at
// fixed second-offsets, so relative timestamps ("2m", "3h", "1d") are stable
// across runs and equal in all three tracks + both app shells.

export interface TwitterUser {
  name: string
  handle: string
}

export interface Tweet {
  id: string
  author: TwitterUser
  text: string
  ts: number // epoch seconds (NOW_TS - offset)
  likes: number
  retweets: number
  replyTo: string | null
  pending?: boolean
}

// Fixed "current time" anchor (epoch seconds). Components receive this as a
// prop and never call Date.now(), so every render is deterministic.
export const NOW_TS = 1750000000

export const ME: TwitterUser = { name: 'You', handle: 'you' }

const AUTHORS: TwitterUser[] = [
  { name: 'Ada Lovelace', handle: 'ada' },
  { name: 'Grace Hopper', handle: 'hopper' },
  { name: 'Alan Turing', handle: 'turing' },
  { name: 'Katherine Johnson', handle: 'katherine' },
  { name: 'Edsger Dijkstra', handle: 'dijkstra' },
  { name: 'Barbara Liskov', handle: 'liskov' },
  { name: 'Donald Knuth', handle: 'knuth' },
  { name: 'Margaret Hamilton', handle: 'margaret' },
]

const TOPICS = [
  'Compilers are just very opinionated translators.',
  'Hot take: whitespace is syntax and that is fine.',
  'Shipping a demo today. What could possibly go wrong?',
  'The best bug report is a failing test.',
  'Reading old papers so you do not have to.',
  'Every abstraction leaks; pick the leaks you can live with.',
  'Rewrote it. It is slower now, but prettier.',
  'Benchmarks are a genre of fiction I enjoy.',
]

// Hand-authored head of the feed: varied timestamp buckets ("now", minutes,
// hours, days), the deterministic #fail tweet (like/retweet rollback), and
// two threads (t1 has 3 replies, t2 has 2).
const HEAD: Tweet[] = [
  {
    id: 't1',
    author: AUTHORS[0],
    text: 'Just shipped the tri-track compiler pipeline. Same source, three targets, zero drift.',
    ts: NOW_TS - 30, // "now"
    likes: 12,
    retweets: 4,
    replyTo: null,
  },
  {
    id: 't2',
    author: AUTHORS[1],
    text: 'A ship in port is safe, but that is not what ships are built for. Ship the demo.',
    ts: NOW_TS - 120, // "2m"
    likes: 48,
    retweets: 17,
    replyTo: null,
  },
  {
    id: 't3',
    author: AUTHORS[2],
    text: 'Deploy pipeline is haunted again #fail — every optimistic update on this one rolls back.',
    ts: NOW_TS - 2700, // "45m"
    likes: 7,
    retweets: 1,
    replyTo: null,
  },
  {
    id: 't4',
    author: AUTHORS[3],
    text: 'Checked the trajectory by hand. The computer agrees with me, which is a relief for the computer.',
    ts: NOW_TS - 10800, // "3h"
    likes: 91,
    retweets: 33,
    replyTo: null,
  },
  {
    id: 't5',
    author: AUTHORS[4],
    text: 'Testing shows the presence, not the absence of bugs. Optimistic UI shows the presence of hope.',
    ts: NOW_TS - 93600, // "1d"
    likes: 64,
    retweets: 21,
    replyTo: null,
  },
  {
    id: 't6',
    author: AUTHORS[5],
    text: 'Substitutability is a promise. Keep it and your users never notice; break it and they never forget.',
    ts: NOW_TS - 432000, // "5d"
    likes: 39,
    retweets: 9,
    replyTo: null,
  },
  {
    id: 't7',
    author: AUTHORS[6],
    text: 'Beware of bugs in the above code; I have only proved it correct, not tried it.',
    ts: NOW_TS - 5400, // "1h"
    likes: 120,
    retweets: 55,
    replyTo: null,
  },
  {
    id: 't8',
    author: AUTHORS[7],
    text: 'There was no choice but to be pioneers. The onboard software had to work the first time.',
    ts: NOW_TS - 172800, // "2d"
    likes: 84,
    retweets: 40,
    replyTo: null,
  },
]

// Replies (thread view): t1 gets 3, t2 gets 2. Replies never appear in the
// top-level feed; they render only inside the thread view.
const REPLIES: Tweet[] = [
  {
    id: 'r1',
    author: AUTHORS[1],
    text: 'Zero drift is a bold claim. Did the parity suite agree?',
    ts: NOW_TS - 20,
    likes: 3,
    retweets: 0,
    replyTo: 't1',
  },
  {
    id: 'r2',
    author: AUTHORS[4],
    text: 'Three targets means three times the ways to be wrong. Impressive that it is not.',
    ts: NOW_TS - 15,
    likes: 5,
    retweets: 1,
    replyTo: 't1',
  },
  {
    id: 'r3',
    author: AUTHORS[6],
    text: 'Now prove it correct. Or at least try it.',
    ts: NOW_TS - 10,
    likes: 8,
    retweets: 2,
    replyTo: 't1',
  },
  {
    id: 'r4',
    author: AUTHORS[0],
    text: 'Port is where the interesting corrosion happens, though.',
    ts: NOW_TS - 60,
    likes: 2,
    retweets: 0,
    replyTo: 't2',
  },
  {
    id: 'r5',
    author: AUTHORS[3],
    text: 'Shipping it. Trajectory verified.',
    ts: NOW_TS - 45,
    likes: 4,
    retweets: 1,
    replyTo: 't2',
  },
]

// Programmatic tail: 58 more top-level tweets → 66 total (≥ 60 required for
// the infinite feed). All values derived from the index — deterministic.
const TAIL: Tweet[] = Array.from({ length: 58 }, (_, i) => ({
  id: `t${i + 9}`,
  author: AUTHORS[i % AUTHORS.length],
  text: `${TOPICS[i % TOPICS.length]} (#${i + 9})`,
  ts: NOW_TS - 21600 - i * 360, // 6h .. ~11.7h ago
  likes: (i * 7) % 90,
  retweets: (i * 3) % 40,
  replyTo: null,
}))

export const TWEETS: Tweet[] = [...HEAD, ...REPLIES, ...TAIL]

// Convenience: the props every shell page passes to the TwitterApp island.
export const twitterFixture = {
  tweets: TWEETS,
  now_ts: NOW_TS,
  me: ME,
}
