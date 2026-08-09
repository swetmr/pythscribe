// jsdom has no IntersectionObserver — install a controllable mock so the
// VideoFeed / YouTubeApp parity suites can drive infinite scroll
// deterministically. Test-only helper (not a component; not globbed by
// vitest.config.ts's include pattern).

export interface MockIO {
  cb: (entries: Array<{ isIntersecting: boolean }>, obs: MockIO) => void
  observed: Element[]
  observe(el: Element): void
  unobserve(el: Element): void
  disconnect(): void
  trigger(isIntersecting: boolean): void
}

export function installIntersectionObserverMock(): { instances: MockIO[] } {
  const instances: MockIO[] = []

  class MockIntersectionObserver implements MockIO {
    cb: MockIO['cb']
    observed: Element[] = []
    constructor(cb: MockIO['cb'], _opts?: unknown) {
      this.cb = cb
      instances.push(this)
    }
    observe(el: Element) {
      this.observed.push(el)
    }
    unobserve(el: Element) {
      this.observed = this.observed.filter((e) => e !== el)
    }
    disconnect() {
      this.observed = []
    }
    trigger(isIntersecting: boolean) {
      this.cb([{ isIntersecting }], this)
    }
  }

  ;(globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = MockIntersectionObserver
  return { instances }
}
