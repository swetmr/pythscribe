// Radix Dialog — TSX reference (oracle).
import * as React from 'react'
import * as Dialog from '@radix-ui/react-dialog'

export function DialogDemo() {
  const [isOpen, setIsOpen] = React.useState(false)
  const [refOk, setRefOk] = React.useState(false)
  const triggerRef = React.useRef<HTMLButtonElement | null>(null)
  const contentProps = { 'data-testid': 'content' }

  React.useEffect(() => {
    setRefOk(triggerRef.current !== null)
  }, [])

  return (
    <Dialog.Root open={isOpen} onOpenChange={setIsOpen}>
      <Dialog.Trigger asChild ref={triggerRef}>
        <button data-testid="trigger">open dialog</button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Content {...contentProps}>
          <Dialog.Title>Settings</Dialog.Title>
          <Dialog.Description>demo dialog</Dialog.Description>
          <p data-testid="state">{isOpen ? 'open' : 'closed'}</p>
          <p data-testid="ref-state">{refOk ? 'ref-attached' : 'ref-missing'}</p>
          <Dialog.Close asChild>
            <button data-testid="close">close</button>
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
