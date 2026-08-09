// Radix Checkbox — TSX reference (oracle).
import * as React from 'react'
import * as Checkbox from '@radix-ui/react-checkbox'

export function CheckboxDemo() {
  const [checked, setChecked] = React.useState<boolean | 'indeterminate'>(false)
  return (
    <div>
      <Checkbox.Root
        data-testid="cb"
        checked={checked}
        onCheckedChange={(value) => setChecked(value)}
      >
        <Checkbox.Indicator data-testid="cb-indicator">on</Checkbox.Indicator>
      </Checkbox.Root>
      <p data-testid="cb-state">{checked === true ? 'yes' : 'no'}</p>
    </div>
  )
}
