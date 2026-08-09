// framer-motion — behavioral parity: TSX reference vs .ps twin.
// Assertions are jsdom-realistic: inline style opacity driven by the
// animation loop, AnimatePresence mount/unmount. No layout measurements
// (jsdom rects are zero).
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'

import { MotionDemo as TsxMotionDemo } from '../references/motion_demo'
import { MotionDemo as PsMotionDemo } from '../components/motion_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxMotionDemo],
  ['ps', PsMotionDemo],
] as const

describe.each(tracks)('framer-motion [%s]', (_track, Demo) => {
  it('motion.div renders, initial opacity applies, animate drives it to 1', async () => {
    render(createElement(Demo as any))
    const box = screen.getByTestId('box')
    expect(box.textContent).toBe('animated box')
    // initial={opacity: 0} is applied synchronously as inline style.
    expect(box.style.opacity).toBe('0')
    // animate={opacity: 1} completes (duration 0.01s).
    await waitFor(() => expect(box.style.opacity).toBe('1'))
  })

  it('AnimatePresence mounts and unmounts the conditional child', async () => {
    const user = userEvent.setup()
    render(createElement(Demo as any))
    expect(screen.getByTestId('presence')).toBeTruthy()

    await user.click(screen.getByTestId('toggle'))
    await waitFor(() => expect(screen.queryByTestId('presence')).toBeNull())

    await user.click(screen.getByTestId('toggle'))
    await screen.findByTestId('presence')
  })
})
