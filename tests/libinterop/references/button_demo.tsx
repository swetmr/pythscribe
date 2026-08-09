// shadcn-style Button (cva + clsx + tailwind-merge) — TSX reference (oracle).
import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

function cn(...inputs: any[]) {
  return twMerge(clsx(inputs))
}

const buttonVariants = cva(
  'inline-flex items-center rounded-md text-sm font-medium',
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground',
        destructive: 'bg-destructive text-destructive-foreground',
        outline: 'border border-input bg-background',
      },
      size: {
        default: 'h-10 px-4 py-2',
        sm: 'h-9 px-3',
        lg: 'h-11 px-8',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
)

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> &
  VariantProps<typeof buttonVariants>

export function Button({ variant, size, className, children, ...rest }: ButtonProps) {
  return (
    <button className={cn(buttonVariants({ variant, size }), className)} {...rest}>
      {children}
    </button>
  )
}

export function ButtonDemo() {
  return (
    <div>
      <Button>Default</Button>
      <Button variant="destructive" size="sm">
        Delete
      </Button>
      <Button
        variant="outline"
        size="lg"
        className="h-12 px-10 custom-extra"
        disabled
        data-testid="btn-merged"
      >
        Merged
      </Button>
    </div>
  )
}
