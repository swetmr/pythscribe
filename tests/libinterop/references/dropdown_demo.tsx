// Radix DropdownMenu — TSX reference (oracle).
import * as React from 'react'
import * as Menu from '@radix-ui/react-dropdown-menu'

export function DropdownDemo() {
  const [picked, setPicked] = React.useState('none')
  return (
    <div>
      <Menu.Root>
        <Menu.Trigger asChild>
          <button data-testid="menu-trigger">menu</button>
        </Menu.Trigger>
        <Menu.Portal>
          <Menu.Content>
            <Menu.Item
              data-testid="item-alpha"
              onSelect={(e) => {
                e.preventDefault()
                setPicked('alpha')
              }}
            >
              Alpha
            </Menu.Item>
            <Menu.Item data-testid="item-beta" onSelect={() => setPicked('beta')}>
              Beta
            </Menu.Item>
          </Menu.Content>
        </Menu.Portal>
      </Menu.Root>
      <p data-testid="picked">{picked}</p>
    </div>
  )
}
