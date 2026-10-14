import { useEffect, useRef, useState } from 'react'
import type { Dispatch } from 'react'
import { debounce } from '../state'
import type { Action, AppState } from '../state'
import * as bridge from '../bridge'
import type { ActionInfo } from '../bridge'
import { Results } from './Results'
import { SettingsOverlay } from './SettingsOverlay'
import { Toast } from './Toast'
import { currentBG } from '../theme'

interface Props {
  state: AppState
  dispatch: Dispatch<Action>
}

/**
 * The launcher panel: search field + results list, keyboard nav, and the
 * window-level hooks the Rust host drives via evaluate_script. All search
 * fetches are last-write-wins with a request-sequence guard so stale responses
 * can never overtake a newer query, and no identical URL is ever in flight
 * twice (WebKit cancels duplicate in-flight fetches silently).
 */
export function Panel({ state, dispatch }: Props) {
  const { query, items, sel, settingsOpen, settingsDoc, toast } = state
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const stateRef = useRef(state)
  stateRef.current = state

  // Universal Actions menu (Alfred: Tab shows actions of the selected result)
  const [acts, setActs] = useState<ActionInfo[] | null>(null)
  const [actSel, setActSel] = useState(0)
  const actsRef = useRef<ActionInfo[] | null>(null)
  const actSelRef = useRef(0)
  const actBusyRef = useRef(false)
  actsRef.current = acts
  actSelRef.current = actSel
  const closeActs = (): void => {
    actsRef.current = null
    actBusyRef.current = false
    setActs(null)
    setActSel(0)
  }

  // any result/selection change dismisses the menu, like Alfred
  useEffect(() => {
    closeActs()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, sel])

  const qidRef = useRef(0)
  const lastQRef = useRef('')

  // typed-query history (REA 2.11: Ctrl+Up/Down scrolls; Up with empty query)
  const histRef = useRef<string[] | null>(null)
  const histIdx = useRef(-1)
  const histDraft = useRef('')
  // empty-box Space file mode (Alfred keyword switch; REA 2.11)
  const filesModeRef = useRef(false)
  const [filesMode, setFilesMode] = useState(false)
  const resetHist = (): void => {
    histIdx.current = -1
    histDraft.current = ''
  }
  const searchRef = useRef<(q: string) => void>(() => undefined)

  searchRef.current = (q: string): void => {
    if (!q) {
      qidRef.current += 1
      lastQRef.current = ''
      dispatch({ type: 'ITEMS', items: [] })
      return
    }
    if (q === lastQRef.current && !filesModeRef.current) return // same query, possibly already in flight
    lastQRef.current = q
    const id = (qidRef.current += 1)
    bridge.search(q, filesModeRef.current).then(
      (v) => {
        if (id === qidRef.current) dispatch({ type: 'ITEMS', items: Array.isArray(v) ? v : [] })
      },
      () => {
        if (id === qidRef.current) dispatch({ type: 'ITEMS', items: [] })
      },
    )
  }

  const debounced = useRef(debounce((q: string) => { searchRef.current(q) }, 60)).current

  // input focus on first paint (host also calls rfReset on every summon)
  useEffect(() => {
    if (inputRef.current) inputRef.current.focus()
  }, [])

  // query typing: debounce ~60ms, last write wins; empty clears immediately
  useEffect(() => {
    if (query) debounced(query)
    else {
      debounced.cancel()
      searchRef.current('')
    }
  }, [query, debounced])

  // selftest telemetry: report the number of rows actually rendered, after
  // every render (matches what the host probes with querySelectorAll('.i'))
  useEffect(() => {
    bridge.probeRows(items.length)
  })

  // smooth height sync: ResizeObserver -> GET /resize?h=<scrollHeight>
  useEffect(() => {
    const el = rootRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => {
      bridge.resize(el.scrollHeight)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // theme probe once after first paint: /probe?bg=<lowercase hex, no '#'>
  useEffect(() => {
    const t = setTimeout(() => {
      void bridge.probeBG(currentBG())
    }, 50)
    return () => clearTimeout(t)
  }, [])

  const openSettings = (): void => {
    const s = stateRef.current
    if (s.settingsOpen) {
      dispatch({ type: 'SETTINGS_CLOSE' })
      return
    }
    if (s.settingsDoc) {
      dispatch({ type: 'SETTINGS_OPEN', doc: s.settingsDoc })
      if (inputRef.current) inputRef.current.blur()
      return
    }
    bridge.settingsGet().then(
      (doc) => {
        dispatch({ type: 'SETTINGS_OPEN', doc })
        if (inputRef.current) inputRef.current.blur()
      },
      () => dispatch({ type: 'TOAST', text: 'settings: could not load' }),
    )
  }
  const openSettingsRef = useRef(openSettings)
  openSettingsRef.current = openSettings

  // ---- window hooks driven by the Rust host (evaluate_script) ----
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>
    w.rfSearch = (q: string): void => {
      const s = String(q == null ? '' : q)
      dispatch({ type: 'QUERY', query: s })
      searchRef.current(s) // run immediately, not debounced
    }
    w.rfReset = (): void => {
      filesModeRef.current = false
      setFilesMode(false)
      dispatch({ type: 'CLEAR' })
      if (inputRef.current) inputRef.current.focus()
    }
    w.rfApply = (arr: unknown): void => {
      dispatch({ type: 'ITEMS', items: Array.isArray(arr) ? (arr as AppState['items']) : [] })
    }
    w.rfExec = (n: number): void => {
      void bridge.exec(Number(n) || 0)
    }
    w.rfProbe = (n: number): void => {
      bridge.probeRows(Number(n) || 0)
    }
    w.rfTheme = (): void => {
      void bridge.probeBG(currentBG())
    }
    w.rfLargeType = (text: string, size?: number): void => {
      const n = Number(size)
      dispatch({ type: 'LARGE', text: String(text), size: n > 0 ? n : undefined })
    }
    w.rfLargeTypeOff = (): void => {
      dispatch({ type: 'LARGE', text: null })
    }
    w.__rfApplyLargeType = (text: string | null): void => {
      dispatch({ type: 'LARGE', text: text == null ? null : String(text) })
    }
    // hooks stay installed for the page lifetime; nothing to clean up
  }, [dispatch])

  // ---- keyboard ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      const s = stateRef.current
      const menu = actsRef.current
      if (menu && menu.length > 0) {
        // while the actions menu is open it owns navigation keys
        if (e.key === 'Escape') { e.preventDefault(); closeActs(); return }
        if (e.key === 'Tab' || e.key === 'ArrowDown') {
          e.preventDefault()
          const n = (actsRef.current ?? []).length
          setActSel((i) => (i + 1) % n)
          return
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          const n = (actsRef.current ?? []).length
          setActSel((i) => (i - 1 + n) % n)
          return
        }
        if (e.key === 'Enter' || e.key === 'ArrowRight') {
          // Enter / Right Arrow execute the highlighted action (Alfred behavior)
          e.preventDefault()
          const a = (actsRef.current ?? [])[actSelRef.current]
          if (a) void bridge.runAction(s.sel, a.id)
          closeActs()
          return
        }
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        if (s.largeType != null) dispatch({ type: 'LARGE', text: null })
        else if (s.settingsOpen) dispatch({ type: 'SETTINGS_CLOSE' })
        else if (filesModeRef.current && s.query === '') {
          filesModeRef.current = false
          setFilesMode(false)
        } else void bridge.hide()
        return
      }
      if (e.metaKey && e.key === ',') {
        e.preventDefault()
        // Alfred: Cmd+, opens the Preferences window; only when the host
        // cannot open one (legacy assets) fall back to the in-panel overlay.
        void bridge.prefs().then((ok) => {
          if (!ok) openSettingsRef.current()
        })
        return
      }
      if (e.metaKey && (e.key === 'y' || e.key === 'Y')) {
        e.preventDefault()
        if (!s.settingsOpen && s.items.length > 0) void bridge.ql(s.sel)
        return
      }
      if (e.metaKey && (e.key === 'l' || e.key === 'L')) {
        // REA 2.11: Cmd+L = Large Type on the selected result's text
        e.preventDefault()
        const it = s.items[s.sel]
        if (it && !s.settingsOpen) dispatch({ type: 'LARGE', text: it.t })
        return
      }
      if (e.metaKey && /^[1-9]$/.test(e.key)) {
        e.preventDefault()
        const n = Number(e.key) - 1
        if (!s.settingsOpen && s.items[n]) void bridge.exec(n) // cmd+N = row N-1
        return
      }
      if (s.settingsOpen) return // overlay owns keys here

      // REA 2.11: Up with an empty query starts history scroll (last typed)
      if (e.key === 'ArrowUp' && !e.ctrlKey && !e.metaKey && !e.altKey && s.query === '' && s.items.length === 0) {
        e.preventDefault()
        void (async () => {
          if (!histRef.current) histRef.current = await bridge.historyGet(50)
          const h = histRef.current
          if (h && h.length > 0) {
            histDraft.current = ''
            histIdx.current = 0
            dispatch({ type: 'QUERY', query: h[0] })
          }
        })()
        return
      }

      // Ctrl+Up / Ctrl+Down: scroll typed-query history (REA 2.11)
      if (e.ctrlKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.metaKey && !e.altKey) {
        e.preventDefault()
        void (async () => {
          if (!histRef.current) histRef.current = await bridge.historyGet(50)
          const h = histRef.current
          if (!h || h.length === 0) return
          if (e.key === 'ArrowUp') {
            if (histIdx.current < 0) {
              histDraft.current = s.query
              histIdx.current = 0
            } else {
              histIdx.current = Math.min(histIdx.current + 1, h.length - 1)
            }
            dispatch({ type: 'QUERY', query: h[histIdx.current] })
          } else if (histIdx.current >= 0) {
            histIdx.current -= 1
            dispatch({ type: 'QUERY', query: histIdx.current < 0 ? histDraft.current : h[histIdx.current] })
          }
        })()
        return
      }

      if (e.key === ' ' && !e.metaKey && !e.ctrlKey && !e.altKey && s.query === '') {
        // empty-box Space toggles file-only search (Alfred's keyword switch)
        e.preventDefault()
        filesModeRef.current = !filesModeRef.current
        setFilesMode(filesModeRef.current)
        if (!filesModeRef.current) searchRef.current('')
        return
      }
      if (e.key === 'Backspace' && !e.metaKey && !e.ctrlKey && s.query === '' && filesModeRef.current) {
        e.preventDefault()
        filesModeRef.current = false
        setFilesMode(false)
        return
      }
      if (e.altKey && e.key === 'ArrowRight') {
        // REA 2.11: Option+Right also opens the universal-actions panel
        if (s.items.length > 0 && !actBusyRef.current) {
          e.preventDefault()
          actBusyRef.current = true
          void bridge.actionsFor(s.sel).then((list) => {
            actBusyRef.current = false
            if (list.length > 0) {
              actsRef.current = list
              setActs(list)
              setActSel(0)
            }
          })
        }
        return
      }

      if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && s.items.length > 0) {
        e.preventDefault()
        if (!actBusyRef.current) {
          actBusyRef.current = true
          void bridge.actionsFor(s.sel).then((list) => {
            actBusyRef.current = false
            if (list.length > 0) {
              actsRef.current = list
              setActs(list)
              setActSel(0)
            }
          })
        }
        return
      }
      if (s.items.length > 0 && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')
          && !e.metaKey && !e.ctrlKey && !e.altKey) {
        // REA 2.11 arrowsForFolderNavigation: Right opens the selected file/
        // folder in Finder, Left reveals the enclosing folder; panel stays up.
        const it = s.items[s.sel]
        if (it && (it.k === 'folder' || it.k === 'document')) {
          e.preventDefault()
          void bridge.navigate(s.sel, e.key === 'ArrowLeft')
        }
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        dispatch({ type: 'MOVE', delta: 1 })
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        dispatch({ type: 'MOVE', delta: -1 })
      } else if (e.key === 'Enter') {
        if (s.items.length > 0) {
          e.preventDefault()
          void bridge.exec(s.sel)
        }
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [dispatch])

  return (
    <div ref={rootRef} className={'rf-root' + (settingsOpen ? ' rf-settings-open' : '')}>
      <div className={'rf-field' + (filesMode ? ' rf-filemode' : '')}>
        {filesMode && <div className="rf-filemode-badge">Files</div>}
        <input
          ref={inputRef}
          id="q"
          type="text"
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          placeholder=""
          aria-label="Search"
          role="combobox"
          aria-expanded={query.length > 0 && items.length > 0}
          aria-controls={items.length > 0 ? 'rf-list' : undefined}
          aria-autocomplete="list"
          value={query}
          onChange={(e) => {
            resetHist()
            dispatch({ type: 'QUERY', query: e.target.value })
          }}
        />
      </div>

      {items.length > 0 && (
        <Results
          id="rf-list"
          items={items}
          sel={sel}
          onSel={(i) => dispatch({ type: 'SEL', index: i })}
          onExec={(i) => {
            void bridge.exec(i)
          }}
        />
      )}

      {acts && acts.length > 0 && (
        <div className="rf-actmenu" role="menu" aria-label="Universal Actions">
          {acts.map((a, i) => (
            <div
              key={a.id}
              className={'rf-actrow' + (i === actSel ? ' rf-actrow-sel' : '')}
              role="menuitem"
              onMouseEnter={() => setActSel(i)}
              onClick={() => {
                void bridge.runAction(state.sel, a.id)
                closeActs()
              }}
            >
              {a.label}
            </div>
          ))}
        </div>
      )}

      {settingsOpen && settingsDoc && (
        <SettingsOverlay
          doc={settingsDoc}
          dispatch={dispatch}
          onToast={(t) => dispatch({ type: 'TOAST', text: t })}
        />
      )}

      <Toast text={toast} />
    </div>
  )
}
