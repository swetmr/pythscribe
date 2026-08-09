// framer-motion — TSX reference (oracle).
import * as React from 'react'
import { motion, AnimatePresence } from 'framer-motion'

export function MotionDemo() {
  const [shown, setShown] = React.useState(true)
  return (
    <div>
      <motion.div
        data-testid="box"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.01 }}
        layout
      >
        animated box
      </motion.div>
      <button data-testid="toggle" onClick={() => setShown(!shown)}>
        toggle
      </button>
      <AnimatePresence>
        {shown ? (
          <motion.p data-testid="presence" exit={{ opacity: 0 }} transition={{ duration: 0.01 }}>
            present
          </motion.p>
        ) : null}
      </AnimatePresence>
    </div>
  )
}
