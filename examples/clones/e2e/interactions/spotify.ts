import { expect } from '@playwright/test'
import { PLAYLISTS, SHUFFLE_SEED, seededShuffle } from '../../shared/spotify/fixtures'
import type { InteractionScript } from './types'

// Spotify — queue building via row menus, deterministic seeded shuffle,
// real <audio> playback, keyboard volume. Playback wall-clock state lives in
// element PROPERTIES (currentTime) which the snapshot ignores, except the
// seek bar's live @value — excluded via volatileValueTestIds. `loop = true`
// is set at ready (same trick as spotify.spec.ts) so the ~1s sample never
// fires `ended` and auto-advances the queue mid-diff.

const tracks = PLAYLISTS[0].tracks
const picks = [tracks[0], tracks[4], tracks[2], tracks[6]]
const shuffled = seededShuffle(picks, SHUFFLE_SEED).map((t) => t.title)

export const spotifyScript: InteractionScript = {
  clone: 'spotify',
  // player-audio: the /spotify production wrapper deliberately injects real
  // mp3 srcs (vite/src/pages/clones/Spotify.tsx "demo-page ONLY real-music
  // injection") while the oracle keeps /media/sample.wav — an INTENTIONAL
  // cross-track input divergence, so the audio element's src/duration-derived
  // state is masked, not diffed.
  volatileValueTestIds: ['player-seek', 'player-elapsed', 'player-total', 'player-audio'],
  ready: async (page) => {
    await expect(page.getByTestId('player-track-title')).toHaveText('Nothing playing')
    await page.evaluate(() => {
      const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
      a.loop = true
    })
  },
  steps: [
    {
      name: 'build a 4-track queue from the row menus',
      run: async (p) => {
        for (const t of picks) {
          await p.getByTestId(`track-row-${t.id}`).hover()
          await p.getByTestId(`track-menu-${t.id}`).click()
          await p.getByTestId(`queue-add-${t.id}`).click()
        }
      },
      settle: async (p) => {
        await expect(p.locator('[data-testid="queue-item"]')).toHaveText(picks.map((t) => t.title))
      },
    },
    {
      name: 'seeded shuffle reorders the queue deterministically',
      run: (p) => p.getByTestId('player-shuffle').click(),
      settle: async (p) => {
        await expect(p.getByTestId('player-shuffle')).toHaveAttribute('aria-pressed', 'true')
        await expect(p.locator('[data-testid="queue-item"]')).toHaveText(shuffled)
      },
    },
    {
      name: 'play starts the shuffled queue (real audio)',
      run: (p) => p.getByTestId('player-play').click(),
      settle: async (p) => {
        await expect(p.getByTestId('player-track-title')).toHaveText(shuffled[0])
        await p.waitForFunction(() => {
          const a = document.querySelector('[data-testid="player-audio"]') as HTMLAudioElement
          return a && !a.paused && a.currentTime > 0
        })
      },
    },
    {
      name: 'next advances the queue while playing',
      run: (p) => p.getByTestId('player-next').click(),
      settle: async (p) => {
        await expect(p.getByTestId('player-track-title')).toHaveText(shuffled[1])
      },
    },
    {
      name: 'pause freezes playback',
      run: (p) => p.getByTestId('player-play').click(),
      settle: async (p) => {
        await expect(p.getByTestId('player-play')).toHaveAccessibleName('Play')
      },
    },
    {
      name: 'ArrowLeft on the volume slider steps volume down',
      run: async (p) => {
        await p.getByTestId('player-volume').focus()
        await p.keyboard.press('ArrowLeft')
      },
      settle: async (p) => {
        await expect(p.getByTestId('player-volume')).toHaveValue('0.79')
      },
    },
  ],
}
