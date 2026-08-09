// Radix Checkbox — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'

import { CheckboxDemo as TsxCheckboxDemo } from '../references/checkbox_demo'
import { CheckboxDemo as PsCheckboxDemo } from '../components/checkbox_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxCheckboxDemo],
  ['ps', PsCheckboxDemo],
] as const

describe.each(tracks)('Radix Checkbox [%s]', (_track, Demo) => {
  it('toggles checked state through onCheckedChange', async () => {
    const user = userEvent.setup()
    render(createElement(Demo as any))

    const cb = screen.getByTestId('cb')
    expect(cb.getAttribute('aria-checked')).toBe('false')
    expect(screen.queryByTestId('cb-indicator')).toBeNull()
    expect(screen.getByTestId('cb-state').textContent).toBe('no')

    await user.click(cb)
    await waitFor(() => expect(cb.getAttribute('aria-checked')).toBe('true'))
    expect(screen.getByTestId('cb-indicator')).not.toBeNull()
    expect(screen.getByTestId('cb-state').textContent).toBe('yes')

    await user.click(cb)
    await waitFor(() => expect(cb.getAttribute('aria-checked')).toBe('false'))
    expect(screen.queryByTestId('cb-indicator')).toBeNull()
    expect(screen.getByTestId('cb-state').textContent).toBe('no')
  })
})
