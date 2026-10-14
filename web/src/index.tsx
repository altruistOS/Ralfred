import { useEffect, useReducer, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { initialState, reducer } from './state'
import { Panel } from './components/Panel'
import { LargeTypeOverlay } from './components/LargeTypeOverlay'
import { Preferences } from './components/Preferences'
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

/**
 * Hash routing: '' / '#' -> the launcher panel (exactly as before, the only
 * view the host's panel window ever uses); '#prefs' -> the full-window
 * preferences view the host opens in its own native window. In prefs mode the
 * Panel component is not mounted at all, so no ResizeObserver is installed
 * and no /resize requests are ever sent from that window.
 */
function Root() {
  const [prefs, setPrefs] = useState<boolean>(() => location.hash === '#prefs')
  useEffect(() => {
    const onHash = (): void => setPrefs(location.hash === '#prefs')
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
  return prefs ? <Preferences /> : <App />
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
createRoot(mount).render(<Root />)
