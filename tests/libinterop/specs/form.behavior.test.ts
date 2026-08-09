// react-hook-form — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'

import { FormDemo as TsxFormDemo } from '../references/form_demo'
import { FormDemo as PsFormDemo } from '../components/form_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxFormDemo],
  ['ps', PsFormDemo],
] as const

describe.each(tracks)('react-hook-form [%s]', (_track, Demo) => {
  it('submit with empty fields renders both validation errors', async () => {
    const user = userEvent.setup()
    render(createElement(Demo as any))

    expect(screen.queryByTestId('email-error')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect((await screen.findByTestId('email-error')).textContent).toBe('Email required')
    expect(screen.getByTestId('nickname-error').textContent).toBe('Nickname required')
    expect(screen.getByTestId('submitted').textContent).toBe('none')
  })

  it('valid submit clears errors and delivers the data', async () => {
    const user = userEvent.setup()
    render(createElement(Demo as any))

    await user.type(screen.getByTestId('email'), 'ada@lovelace.dev')
    await user.type(screen.getByTestId('nickname'), 'ada')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    await waitFor(() =>
      expect(screen.getByTestId('submitted').textContent).toBe('ada@lovelace.dev|ada'),
    )
    expect(screen.queryByTestId('email-error')).toBeNull()
    expect(screen.queryByTestId('nickname-error')).toBeNull()
  })
})
