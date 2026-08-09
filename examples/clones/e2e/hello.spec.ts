import { test, expect } from './lib/parity-test'

// Runs against BOTH apps (see playwright.vite.config.ts / .next.config.ts —
// same spec, different baseURL/webServer). This is the scaffold-proof
// smoke test every future clone's e2e/<clone>.spec.ts should model: hit
// the landing page, follow a nav link, exercise the production route,
// then the /react-reference oracle mirror.

test('landing page links to the HelloCard demo and clone routes', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByTestId('nav-hello')).toBeVisible()
  await expect(page.getByTestId('nav-youtube')).toBeVisible()
  await expect(page.getByTestId('nav-kanban')).toBeVisible()
})

test('HelloCard (production /hello route) renders and toggles like state', async ({ page }) => {
  await page.goto('/hello')
  await expect(page.getByTestId('hello-title')).toHaveText('Hello, Clones!')
  await expect(page.getByTestId('hello-subtitle')).toHaveText('tri-track scaffold proof')

  const likeBtn = page.getByTestId('hello-like')
  await expect(likeBtn).toHaveText('♡ Like')
  await likeBtn.click()
  await expect(likeBtn).toHaveText('♥ Liked')
})

test('HelloCard (React reference /react-reference/hello route) renders and toggles', async ({
  page,
}) => {
  await page.goto('/react-reference/hello')
  await expect(page.getByTestId('hello-title')).toHaveText('Hello, Clones!')

  const likeBtn = page.getByTestId('hello-like')
  await likeBtn.click()
  await expect(likeBtn).toHaveText('♥ Liked')
})

test('every clone route renders real content (no placeholders left)', async ({ page }) => {
  // The scaffold originally asserted a "Coming soon" placeholder here; all
  // six clones are now built, so the inverse is the invariant.
  for (const slug of ['youtube', 'netflix', 'coursera', 'spotify', 'kanban', 'twitter']) {
    await page.goto(`/${slug}`)
    await expect(page.getByTestId('coming-soon')).toHaveCount(0)
  }
})
