import { useEffect, useReducer } from 'react'
import { createRoot } from 'react-dom/client'
import { initialState, reducer } from './state'
import { Panel } from './components/Panel'
import { LargeTypeOverlay } from './components/LargeTypeOverlay'
import { applyTheme } from './theme'
import cssText from './styles.css'

function App() {
  const [state, dispatch] = useReducer(reducer, initialState)

  // toast auto-dismiss
  useEffect(() => {
    if (!state.toast) return
    const t = setTimeout(() => {
      dispatch({ type: 'TOAST', text: null })
    }, 2600)
    return () => clearTimeout(t)
  }, [state.toast])

  return (
    <>
      <Panel state={state} dispatch={dispatch} />
      <LargeTypeOverlay text={state.largeType} size={state.largeTypeSize} />
    </>
  )
}

// CSS ships inside bundle.js as text and is injected here at boot, so dist/
// contains exactly index.html + bundle.js (the Rust host embeds both via
// include_str!/include_bytes! and serves them from ralfred://api/).
const styleEl = document.createElement('style')
styleEl.textContent = cssText
document.head.appendChild(styleEl)

applyTheme()

const mount = document.getElementById('app')
if (!mount) throw new Error('ralfred web: #app not found in index.html')
createRoot(mount).render(<App />)
