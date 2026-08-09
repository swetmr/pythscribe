// Scaffold-shell metadata only — NOT clone content. Both app shells (vite/
// and next/) import this single list so the landing page and route tables
// never drift. When a clone gets built for real, it stays in this list;
// nothing else about this file changes.
export interface CloneMeta {
  slug: string
  name: string
  stretch?: boolean
}

export const CLONES: CloneMeta[] = [
  { slug: 'youtube', name: 'YouTube' },
  { slug: 'netflix', name: 'Netflix' },
  { slug: 'coursera', name: 'Coursera' },
  { slug: 'spotify', name: 'Spotify' },
  { slug: 'kanban', name: 'Kanban' },
  { slug: '2048', name: '2048' },
  { slug: 'tetris', name: 'Tetris', stretch: true },
  { slug: 'twitter', name: 'Twitter', stretch: true },
]
