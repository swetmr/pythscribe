// lucide-react — behavioral parity: TSX reference vs .ps twin.
import { describe, it, expect, afterEach } from 'vitest'
import { render, cleanup, screen } from '@testing-library/react'
import { createElement } from 'react'

import { IconDemo as TsxIconDemo } from '../references/icons_demo'
import { IconDemo as PsIconDemo } from '../components/icons_demo.ps'

afterEach(cleanup)

const tracks = [
  ['tsx', TsxIconDemo],
  ['ps', PsIconDemo],
] as const

describe.each(tracks)('lucide-react icons [%s]', (_track, Demo) => {
  it('static import renders an svg with size + strokeWidth applied', () => {
    render(createElement(Demo as any))
    const icon = screen.getByTestId('static-icon')
    expect(icon.tagName.toLowerCase()).toBe('svg')
    expect(icon.getAttribute('width')).toBe('16')
    expect(icon.getAttribute('height')).toBe('16')
    expect(icon.getAttribute('stroke-width')).toBe('1.5')
  })

  it('dynamic selection from a dict of component factories works', () => {
    render(createElement(Demo as any))
    const icon = screen.getByTestId('dynamic-icon')
    expect(icon.tagName.toLowerCase()).toBe('svg')
    expect(icon.getAttribute('width')).toBe('24')
  })

  it('dynamic selection follows the name prop', () => {
    render(createElement(Demo as any, { name: 'loader' }))
    const icon = screen.getByTestId('dynamic-icon')
    expect(icon.tagName.toLowerCase()).toBe('svg')
    // Loader2 carries its lucide class name; ChevronDown does not.
    expect(icon.getAttribute('class') ?? '').toContain('loader')
  })
})
