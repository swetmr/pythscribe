'use client'
import { useState } from 'react'
import './HelloCard.css'

/**
 * React reference oracle for the clone-demos scaffold proof.
 * Dual-track-paired with HelloCard.ps / HelloCard.psc — all three must
 * render identical DOM for the same props (see HelloCard.test.tsx).
 *
 * This is deliberately trivial: the scaffold ships NO clone content, only
 * proof that the shared tri-track + dual-app-mount pattern works end to
 * end. Real clones (youtube/, netflix/, ...) follow this exact shape.
 */
export interface HelloCardProps {
  title: string
  subtitle?: string
}

export function HelloCard({ title, subtitle }: HelloCardProps) {
  const [liked, setLiked] = useState(false)
  return (
    <div className="hello-card" data-testid="hello-card">
      <h2 data-testid="hello-title">{title}</h2>
      {subtitle && <p data-testid="hello-subtitle">{subtitle}</p>}
      <button
        className="hello-like-btn"
        data-testid="hello-like"
        data-liked={liked}
        onClick={() => setLiked((l) => !l)}
      >
        {liked ? '♥ Liked' : '♡ Like'}
      </button>
    </div>
  )
}

export default HelloCard
