// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md
// "Fixtures-only rule"). Every clone's fixtures.ts follows this shape: named
// exports of plain data, imported by both the tri-track components and the
// vite/next app shells that mount them.

export const helloFixture = {
  title: 'Hello, Clones!',
  subtitle: 'tri-track scaffold proof',
}
