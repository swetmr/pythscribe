import { defineBehaviorSuite, expect, user, screen, textEntries, primaryButton } from './_harness'

// The compose button, tolerant of per-tweet Repost/Reply/Share look-alikes.
const postBtn = () =>
  primaryButton(/tweet|post/i, /^\s*(tweet|post)\s*$/i, /re-?post|reply|share/i) as HTMLButtonElement

// Contract: TweetFeed() — compose textarea, 280 limit + live remaining counter;
// post button disabled when empty; posting prepends a tweet.
defineBehaviorSuite('macro_tweet_composer', 'TweetFeed', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // live remaining counter starts at 280 (empty draft)
  expect(screen.getAllByText(/\b280\b/).length).toBeGreaterThan(0)

  // post/tweet button disabled while the draft is empty
  expect(postBtn().disabled).toBe(true)

  // typing enables posting and decrements the counter
  // tolerance: the composer is the first text-entry control; a feed may render
  // more (per-tweet reply boxes), which used to make getByRole throw
  const box = textEntries()[0]
  await u.type(box, 'BananaPostUnique')
  expect(postBtn().disabled).toBe(false)
  expect(screen.queryByText(/\b280\b/)).toBeNull()

  // posting prepends the new tweet into the feed
  await u.click(postBtn())
  expect(screen.getByText(/BananaPostUnique/i)).toBeTruthy()
  expect(container).toBeTruthy()
})
