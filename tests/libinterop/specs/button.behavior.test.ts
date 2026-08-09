// shadcn Button (cva + clsx + tailwind-merge) — behavioral parity.
// The load-bearing assertion is EXACT final className strings: cva variant
// resolution, clsx flattening, and twMerge conflict-resolution must all
// produce byte-identical output across both tracks.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen } from '@testing-library/react'
import { createElement } from 'react'

import { ButtonDemo as TsxButtonDemo } from '../references/button_demo'
import { ButtonDemo as PsButtonDemo } from '../components/button_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxButtonDemo],
  ['ps', PsButtonDemo],
] as const

describe.each(tracks)('cva/clsx/twMerge Button [%s]', (_track, Demo) => {
  it('default variants resolve', () => {
    render(createElement(Demo as any))
    expect(screen.getByRole('button', { name: 'Default' }).className).toBe(
      'inline-flex items-center rounded-md text-sm font-medium bg-primary text-primary-foreground h-10 px-4 py-2',
    )
  })

  it('explicit variant + size resolve', () => {
    render(createElement(Demo as any))
    expect(screen.getByRole('button', { name: 'Delete' }).className).toBe(
      'inline-flex items-center rounded-md text-sm font-medium bg-destructive text-destructive-foreground h-9 px-3',
    )
  })

  it('twMerge drops conflicting size classes; **rest passes through', () => {
    render(createElement(Demo as any))
    const merged = screen.getByTestId('btn-merged')
    // h-11/px-8 (size=lg) lose to the later h-12/px-10 from className.
    expect(merged.className).toBe(
      'inline-flex items-center rounded-md text-sm font-medium border border-input bg-background h-12 px-10 custom-extra',
    )
    expect((merged as HTMLButtonElement).disabled).toBe(true)
    expect(merged.textContent).toBe('Merged')
  })
})
