import '@testing-library/jest-dom/vitest'

// jsdom shims required by Radix primitives (positioning/menus) and
// framer-motion (reduced-motion media query). All assigned only when
// missing so a future jsdom that implements them wins.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
;(globalThis as any).ResizeObserver ??= ResizeObserverStub as any

// Pointer-capture APIs — probed by Radix menus and @testing-library/user-event.
const elProto = Element.prototype as any
elProto.hasPointerCapture ??= () => false
elProto.setPointerCapture ??= () => {}
elProto.releasePointerCapture ??= () => {}
elProto.scrollIntoView ??= () => {}

;(window as any).matchMedia ??= (query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: () => {},
  removeListener: () => {},
  addEventListener: () => {},
  removeEventListener: () => {},
  dispatchEvent: () => false,
})
