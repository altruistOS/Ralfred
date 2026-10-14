import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import * as bridge from '../bridge'
import type { SettingValue, SettingsDoc } from '../state'
import { Toast } from './Toast'
import { GeneralPane } from './prefs/GeneralPane'
import { AppearancePane } from './prefs/AppearancePane'
import { FeaturesPane } from './prefs/FeaturesPane'
import { WorkflowsPane } from './prefs/WorkflowsPane'
import { AdvancedPane } from './prefs/AdvancedPane'
import { HelpPane } from './prefs/HelpPane'

type PaneId = 'general' | 'appearance' | 'features' | 'workflows' | 'advanced' | 'help'

interface TabDef {
  id: PaneId
  label: string
  icon: ReactNode
}

const TABS: TabDef[] = [
  {
    id: 'general',
    label: 'General',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="12" cy="12" r="3.5" />
        <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.2 5.2l2.2 2.2M16.6 16.6l2.2 2.2M18.8 5.2l-2.2 2.2M7.4 16.6l-2.2 2.2" />
      </svg>
    ),
  },
  {
    id: 'appearance',
    label: 'Appearance',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="12" cy="12" r="8.5" />
        <path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor" stroke="none" />
      </svg>
    ),
  },
  {
    id: 'features',
    label: 'Features',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <path d="M12 4v16M4 12h16" strokeLinecap="round" />
        <path d="M12 4a4 4 0 0 1 4 4v0a3 3 0 0 0 3 3h0a4 4 0 0 1-4 4" opacity="0" />
      </svg>
    ),
  },
  {
    id: 'workflows',
    label: 'Workflows',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="6" cy="6" r="2.6" />
        <circle cx="6" cy="18" r="2.6" />
        <circle cx="18" cy="12" r="2.6" />
        <path d="M8.4 7.1l7.3 3.7M8.4 16.9l7.3-3.7" />
      </svg>
    ),
  },
  {
    id: 'advanced',
    label: 'Advanced',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <path d="M4 7h16M4 12h16M4 17h16" />
        <circle cx="9.5" cy="7" r="2.1" fill="var(--rf-bg)" />
        <circle cx="15" cy="12" r="2.1" fill="var(--rf-bg)" />
        <circle cx="7" cy="17" r="2.1" fill="var(--rf-bg)" />
      </svg>
    ),
  },
  {
    id: 'help',
    label: 'Help',
    icon: (
      <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="12" cy="12" r="8.5" />
        <text x="12" y="16.5" textAnchor="middle" fontSize="11" fontWeight="600" fill="currentColor" stroke="none">
          ?
        </text>
      </svg>
    ),
  },
]

/**
 * Targets for the ⌘F jump overlay. Each entry maps to a pane (tab) and the id
 * of a control that exists once that pane is mounted; Enter focuses it.
 */
interface JumpItem {
  pane: PaneId
  paneLabel: string
  label: string
  id: string
}

const JUMPS: JumpItem[] = [
  { pane: 'general', paneLabel: 'General', label: 'Hotkey', id: 'pf-hotkey' },
  { pane: 'general', paneLabel: 'General', label: 'Start at login', id: 'pf-login' },
  { pane: 'general', paneLabel: 'General', label: 'Version', id: 'pf-version' },
  { pane: 'appearance', paneLabel: 'Appearance', label: 'Themes', id: 'pf-themes-list' },
  { pane: 'appearance', paneLabel: 'Appearance', label: 'Auto-expand snippets', id: 'pf-autoexpand' },
  { pane: 'appearance', paneLabel: 'Appearance', label: 'Max results', id: 'pf-maxresults' },
  { pane: 'appearance', paneLabel: 'Appearance', label: 'Clipboard history items', id: 'pf-clipboard' },
  { pane: 'features', paneLabel: 'Features', label: 'Features', id: 'pf-features-list' },
  { pane: 'features', paneLabel: 'Features', label: 'Enabled', id: 'pf-fe-enabled' },
  { pane: 'workflows', paneLabel: 'Workflows', label: 'Filter workflows', id: 'pf-wf-search' },
  { pane: 'workflows', paneLabel: 'Workflows', label: 'Workflows', id: 'pf-wf-list' },
  { pane: 'advanced', paneLabel: 'Advanced', label: 'Folders to search', id: 'pf-scopes' },
  { pane: 'advanced', paneLabel: 'Advanced', label: 'Web search', id: 'pf-websearch-add' },
  { pane: 'help', paneLabel: 'Help', label: 'Usage & shortcuts', id: 'pf-help' },
]

/** local-only subsequence fuzzy matcher: -1 = no match, lower = better */
function fuzzy(q: string, text: string): number {
  const hay = text.toLowerCase()
  const needle = q.toLowerCase()
  if (!needle) return 0
  const hit = hay.indexOf(needle)
  if (hit >= 0) return hit
  let pos = -1
  let score = 0
  for (let i = 0; i < needle.length; i++) {
    const p = hay.indexOf(needle[i], pos + 1)
    if (p < 0) return -1
    score += pos >= 0 ? p - pos : p
    pos = p
  }
  return score
}

function JumpOverlay(props: {
  onJump: (item: JumpItem) => void
  onClose: () => void
}) {
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (inputRef.current) inputRef.current.focus()
  }, [])

  const matches = useMemo(() => {
    if (!q) return JUMPS.slice(0, 10)
    const scored = JUMPS.map((j) => ({
      j: j,
      s: Math.min(
        fuzzy(q, j.label),
        fuzzy(q, j.paneLabel + ' ' + j.label) < 0 ? -1 : fuzzy(q, j.paneLabel) + 10,
      ),
    }))
    const ok = scored.filter((x) => x.s >= 0)
    ok.sort((a, b) => a.s - b.s)
    return ok.slice(0, 8).map((x) => x.j)
  }, [q])

  useEffect(() => {
    if (sel >= matches.length) setSel(0)
  }, [matches, sel])

  const jump = (item: JumpItem | undefined): void => {
    if (item) props.onJump(item)
    else props.onClose()
  }

  return (
    <div className="pf-overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) props.onClose() }}>
      <div className="pf-hud" role="dialog" aria-label="Jump to setting">
        <input
          ref={inputRef}
          id="pf-jump-input"
          className="pf-hud-input"
          type="text"
          autoComplete="off"
          spellCheck={false}
          placeholder="Search settings…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault()
              setSel((n) => Math.min(n + 1, matches.length - 1))
            } else if (e.key === 'ArrowUp') {
              e.preventDefault()
              setSel((n) => Math.max(n - 1, 0))
            } else if (e.key === 'Enter') {
              e.preventDefault()
              jump(matches[sel])
            }
          }}
        />
        <ul className="pf-hud-list" role="listbox" aria-label="Settings">
          {matches.map((m, i) => (
            <li
              key={m.pane + '|' + m.label}
              className={'pf-hud-row' + (i === sel ? ' sel' : '')}
              role="option"
              aria-selected={i === sel}
              onMouseEnter={() => setSel(i)}
              onClick={() => jump(m)}
            >
              <span>{m.label}</span>
              <span className="pf-hud-pane">{m.paneLabel}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  )
}

/**
 * The full-window Preferences view (URL hash '#prefs'). Left 80px tab bar +
 * right content region, driven entirely by live host APIs; every control is a
 * real fetch/PATCH. No ResizeObserver, no /resize — the window size is owned
 * by the native host.
 */
export function Preferences() {
  const [tab, setTab] = useState<PaneId>('general')
  const [doc, setDoc] = useState<SettingsDoc | null>(null)
  const [values, setValues] = useState<Record<string, SettingValue> | null>(null)
  const [loadError, setLoadError] = useState(false)
  const [overlay, setOverlay] = useState(false)
  const [toast, setToast] = useState<string | null>(null)

  const load = useCallback((): void => {
    setLoadError(false)
    bridge.settingsGet().then(
      (d) => {
        setDoc(d)
        setValues(d.values || {})
      },
      () => setLoadError(true),
    )
  }, [])
  useEffect(() => {
    load()
  }, [load])

  useEffect(() => {
    if (!toast) return
    const t = setTimeout(() => setToast(null), 2600)
    return () => clearTimeout(t)
  }, [toast])

  /** optimistic value set + coalesced PATCH /settings */
  const set = useCallback((key: string, value: SettingValue): void => {
    setValues((v) => (v ? { ...v, [key]: value } : v))
    bridge.settingsPatch({ [key]: value }).then(undefined, (err: unknown) => {
      setToast('settings: ' + (err instanceof Error ? err.message : 'save failed'))
    })
  }, [])

  // window close keys: ⌘W always, Escape when the jump overlay is closed.
  // In prefs mode Escape means "close preferences", like native macOS.
  const overlayRef = useRef(overlay)
  overlayRef.current = overlay
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if ((e.metaKey && (e.key === 'w' || e.key === 'W')) || e.key === 'Escape') {
        if (e.key === 'Escape' && overlayRef.current) {
          e.preventDefault()
          setOverlay(false) // Esc first closes the jump overlay
          return
        }
        if (!e.metaKey || e.key === 'w' || e.key === 'W') {
          e.preventDefault()
          bridge.prefsClose()
          return
        }
      }
      if (e.metaKey && (e.key === 'f' || e.key === 'F')) {
        e.preventDefault()
        setOverlay((o) => !o)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const jump = useCallback((item: JumpItem): void => {
    setOverlay(false)
    setTab(item.pane)
    window.setTimeout(() => {
      const el = document.getElementById(item.id)
      if (el) (el as HTMLElement).focus()
    }, 60)
  }, [])

  let content: ReactNode
  if (loadError) {
    content = (
      <div className="pf-empty-txt">
        Could not load settings.
        <button type="button" className="pf-btn" style={{ marginLeft: 12 }} onClick={load}>
          Retry
        </button>
      </div>
    )
  } else if (!values) {
    content = <div className="pf-empty-txt">Loading settings…</div>
  } else if (!doc) {
    content = <div className="pf-empty-txt">Loading settings…</div>
  } else {
    switch (tab) {
      case 'general':
        content = <GeneralPane values={values} set={set} onToast={setToast} />
        break
      case 'appearance':
        content = <AppearancePane values={values} set={set} onToast={setToast} />
        break
      case 'features':
        content = <FeaturesPane values={values} set={set} onToast={setToast} />
        break
      case 'workflows':
        content = <WorkflowsPane values={values} set={set} onToast={setToast} />
        break
      case 'advanced':
        content = <AdvancedPane values={values} set={set} onToast={setToast} />
        break
      case 'help':
        content = <HelpPane values={values} set={set} onToast={setToast} />
        break
    }
  }

  return (
    <div className="rf-prefs">
      <div className="pf-tabs" role="tablist" aria-label="Preferences">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            className={'pf-tab' + (tab === t.id ? ' active' : '')}
            onClick={() => setTab(t.id)}
          >
            <span className="pf-tab-ico">{t.icon}</span>
            <span className="pf-tab-label">{t.label}</span>
          </button>
        ))}
      </div>
      <div className="pf-content">{content}</div>
      {overlay && <JumpOverlay onJump={jump} onClose={() => setOverlay(false)} />}
      <Toast text={toast} />
    </div>
  )
}
