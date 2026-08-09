// Radix Dialog — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'

import { DialogDemo as TsxDialogDemo } from '../references/dialog_demo'
import { DialogDemo as PsDialogDemo } from '../components/dialog_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxDialogDemo],
  ['ps', PsDialogDemo],
] as const

describe.each(tracks)('Radix Dialog [%s]', (_track, Demo) => {
  it('trigger opens, content renders in a portal, close works, ref attaches', async () => {
    const user = userEvent.setup()
    const { container } = render(createElement(Demo as any))

    expect(screen.queryByTestId('content')).toBeNull()

    await user.click(screen.getByTestId('trigger'))
    const content = await screen.findByTestId('content')
    // Portal: content mounts OUTSIDE the render container (document.body).
    expect(container.contains(content)).toBe(false)
    expect(screen.getByTestId('state').textContent).toBe('open')
    // ref= on the asChild Trigger reached the underlying <button>.
    expect(screen.getByTestId('ref-state').textContent).toBe('ref-attached')

    await user.click(screen.getByTestId('close'))
    await waitFor(() => expect(screen.queryByTestId('content')).toBeNull())
  })
})
