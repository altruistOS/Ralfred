import { useEffect, useRef, useState } from 'react'
import type { PaneProps } from './common'
import { CheckRow, toStr } from './common'

/** named keys that don't map from KeyboardEvent.code by simple stripping */
const SPECIAL: Record<string, string> = {
  Space: 'SPACE',
  Enter: 'ENTER',
  Tab: 'TAB',
  Backspace: 'DELETE',
  Delete: 'FWDDEL',
  ArrowUp: 'UP',
  ArrowDown: 'DOWN',
  ArrowLeft: 'LEFT',
  ArrowRight: 'RIGHT',
  Home: 'HOME',
  End: 'END',
  PageUp: 'PGUP',
  PageDown: 'PGDN',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Backquote: '`',
  Minus: '-',
  Equal: '=',
}

/**
 * Build a display combo like 'ALT+CTRL+SPACE' from a keydown event. Returns
 * null while only modifiers are held down (keep capturing).
 */
function comboFromEvent(e: KeyboardEvent): string | null {
  if (['Control', 'Shift', 'Alt', 'Meta', 'CapsLock'].indexOf(e.key) >= 0) return null
  const mods: string[] = []
  if (e.altKey) mods.push('ALT')
  if (e.ctrlKey) mods.push('CTRL')
  if (e.metaKey) mods.push('CMD')
  if (e.shiftKey) mods.push('SHIFT')
  let key = ''
  if (e.code && SPECIAL[e.code] != null) key = SPECIAL[e.code]
  else if (e.code && /^Key[A-Z]$/.test(e.code)) key = e.code.slice(3)
  else if (e.code && /^Digit([0-9])$/.test(e.code)) key = e.code.slice(5)
  else if (e.code && /^Numpad([0-9])$/.test(e.code)) key = 'NUM' + e.code.slice(6)
  else key = String(e.key).toUpperCase()
  if (!key) return null
  return mods.length > 0 ? mods.join('+') + '+' + key : key
}

/**
 * General pane: hotkey recorder + start-at-login + version line. The recorder
 * owns keydown while capturing (window capture-phase listener) so nothing else
 * on the page — including the prefs-window close key — sees the combo keys.
 */
export function GeneralPane({ values, set }: PaneProps) {
  const [capturing, setCapturing] = useState(false)
  const captRef = useRef(false)
  captRef.current = capturing

  useEffect(() => {
    if (!capturing) return
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault()
      e.stopPropagation() // keep it away from the window close-key handler
      if (e.key === 'Escape') {
        setCapturing(false) // ESC cancels the recording
        return
      }
      const combo = comboFromEvent(e)
      if (combo != null) {
        set('hotkey', combo)
        setCapturing(false)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [capturing, set])

  const hotkey = toStr(values.hotkey).toUpperCase()

  return (
    <div className="pf-pane">
      <div className="pf-title">Hotkey</div>
      <div className="pf-frow">
        <label htmlFor="pf-hotkey">
          Summon Alfred
          <span className="pf-hint-txt">Click, then press the key combination you want. ESC cancels.</span>
        </label>
        <div className="pf-fctl">
          <button
            id="pf-hotkey"
            type="button"
            className={'pf-hotkey' + (capturing ? ' cap' : '')}
            aria-pressed={capturing}
            onClick={() => setCapturing(true)}
            onBlur={() => {
              if (captRef.current) setCapturing(false)
            }}
          >
            {capturing ? 'Type shortcut…  (ESC to cancel)' : hotkey || 'Click to record'}
          </button>
        </div>
      </div>

      <CheckRow
        id="pf-login"
        label="Start at login"
        hint="Launch Ralfred automatically when you log in."
        checked={values.launch_at_login === true || values.launch_at_login === 'true'}
        onChange={(v) => set('launch_at_login', v)}
      />

      <div className="pf-version" id="pf-version" tabIndex={-1}>
        Version {toStr(values.version) || '—'}
      </div>
    </div>
  )
}
