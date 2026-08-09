// @tanstack/react-query — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen } from '@testing-library/react'
import { createElement } from 'react'

import { QueryDemo as TsxQueryDemo } from '../references/query_demo'
import { QueryDemo as PsQueryDemo } from '../components/query_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxQueryDemo],
  ['ps', PsQueryDemo],
] as const

describe.each(tracks)('@tanstack/react-query [%s]', (_track, Demo) => {
  it('useQuery transitions loading → data under QueryClientProvider', async () => {
    render(createElement(Demo as any))
    expect(screen.getByTestId('loading')).toBeTruthy()
    const data = await screen.findByTestId('data')
    expect(data.textContent).toBe('hello from query')
    expect(screen.queryByTestId('loading')).toBeNull()
    expect(screen.queryByTestId('error')).toBeNull()
  })
})
