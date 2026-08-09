// lucide-react icons — TSX reference (oracle).
import * as React from 'react'
import { Check, ChevronDown, Loader2 } from 'lucide-react'

const ICONS = { check: Check, chevron: ChevronDown, loader: Loader2 } as const

export function IconDemo({ name = 'chevron' as keyof typeof ICONS }) {
  const Icon = ICONS[name]
  return (
    <div>
      <Check size={16} strokeWidth={1.5} data-testid="static-icon" />
      <Icon size={24} data-testid="dynamic-icon" />
    </div>
  )
}
