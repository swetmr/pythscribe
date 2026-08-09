import type { ReactNode } from 'react'
import '../../shared/theme.css'

export const metadata = {
  title: 'pyths clone-demos (Next.js)',
}

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
