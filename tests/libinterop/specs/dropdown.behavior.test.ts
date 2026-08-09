// Radix DropdownMenu — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'

import { DropdownDemo as TsxDropdownDemo } from '../references/dropdown_demo'
import { DropdownDemo as PsDropdownDemo } from '../components/dropdown_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxDropdownDemo],
  ['ps', PsDropdownDemo],
] as const

describe.each(tracks)('Radix DropdownMenu [%s]', (_track, Demo) => {
  it('opens on trigger click; onSelect fires; preventDefault keeps menu open', async () => {
    const user = userEvent.setup()
    render(createElement(Demo as any))

    expect(screen.queryByTestId('item-alpha')).toBeNull()
    await user.click(screen.getByTestId('menu-trigger'))
    await screen.findByTestId('item-alpha')

    // Alpha's handler calls e.preventDefault() — selection registers AND
    // the menu stays open (Radix onSelect contract).
    await user.click(screen.getByTestId('item-alpha'))
    expect(screen.getByTestId('picked').textContent).toBe('alpha')
    expect(screen.queryByTestId('item-alpha')).not.toBeNull()

    // Beta's handler does not prevent default — menu closes after select.
    await user.click(screen.getByTestId('item-beta'))
    expect(screen.getByTestId('picked').textContent).toBe('beta')
    await waitFor(() => expect(screen.queryByTestId('item-beta')).toBeNull())
  })
})
