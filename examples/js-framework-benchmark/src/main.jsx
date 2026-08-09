// PythScribe `.ps` track bootstrap — mounts the compiled Main.ps.
import { createRoot } from 'react-dom/client'
import { createElement } from 'react'
import App from './Main.ps'

createRoot(document.getElementById('main')).render(createElement(App))
