import { useEffect, useRef, useState } from 'react'
import { debounce } from '../../state'
import type { PaneProps } from './common'
import { toLines, toMap } from './common'

interface Row {
  id: number
  kw: string
  url: string
}

/**
 * Advanced pane: 'Folders to search' (default_scope, one path per line) and
 * the web_search keyword->URL map editor. Both debounce 400ms to PATCH
 * /settings, and flush pending edits when the pane unmounts (tab change).
 */
export function AdvancedPane({ values, set }: PaneProps) {
  const [scopes, setScopes] = useState<string>(() => toLines(values.default_scope).join('\n'))
  const scopesRef = useRef(scopes)
  scopesRef.current = scopes

  const nextId = useRef(1)
  const [rows, setRows] = useState<Row[]>(() => {
    const map = toMap(values.web_search)
    return Object.keys(map).map((kw) => {
      const id = nextId.current++
      return { id: id, kw: kw, url: map[kw] }
    })
  })
  const rowsRef = useRef(rows)
  rowsRef.current = rows

  const loadedScopes = useRef<string[]>(toLines(values.default_scope))
  const loadedRows = useRef<string>(JSON.stringify(rows.map((r) => [r.kw, r.url])))

  const pushScopes = useRef(
    debounce(() => {
      const lines = scopesRef.current
        .split('\n')
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
      const cur = JSON.stringify(lines)
      if (cur !== JSON.stringify(loadedScopes.current)) {
        loadedScopes.current = lines
        set('default_scope', lines)
      }
    }, 400),
  ).current

  const pushWeb = useRef(
    debounce(() => {
      const map: Record<string, string> = {}
      for (const r of rowsRef.current) {
        const kw = r.kw.trim()
        if (kw.length > 0) map[kw] = r.url.trim()
      }
      const cur = JSON.stringify(rowsRef.current.map((r) => [r.kw.trim(), r.url.trim()]))
      const loaded = loadedRows.current
      if (cur !== loaded) {
        loadedRows.current = cur
        set('web_search', map)
      }
    }, 400),
  ).current

  // flush unsaved edits when the pane unmounts (tab switch / view close)
  useEffect(() => {
    return () => {
      pushScopes.cancel()
      pushWeb.cancel()
      const lines = scopesRef.current
        .split('\n')
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
      if (JSON.stringify(lines) !== JSON.stringify(loadedScopes.current)) {
        loadedScopes.current = lines
        set('default_scope', lines)
      }
      const map: Record<string, string> = {}
      for (const r of rowsRef.current) {
        const kw = r.kw.trim()
        if (kw.length > 0) map[kw] = r.url.trim()
      }
      const cur = JSON.stringify(rowsRef.current.map((r) => [r.kw.trim(), r.url.trim()]))
      if (cur !== loadedRows.current) {
        loadedRows.current = cur
        set('web_search', map)
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const patchRow = (id: number, part: Partial<Row>): void => {
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, ...part } : r)))
    pushWeb()
  }

  return (
    <div className="pf-pane">
      <div className="pf-title">Folders to search</div>
      <textarea
        id="pf-scopes"
        className="pf-input pf-scopes"
        value={scopes}
        spellCheck={false}
        placeholder="/Applications&#10;/Users/you/Documents"
        onChange={(e) => {
          setScopes(e.target.value)
          pushScopes()
        }}
      />
      <div className="pf-note">One path per line. Saved automatically.</div>

      <div className="pf-title" style={{ marginTop: 28 }}>
        Web search keyword → URL
      </div>
      <div className="pf-note">Type '{'kw query'}' in the panel to open the matching URL.</div>
      <div className="pf-webrows" id="pf-websearch">
        {rows.map((r) => (
          <div className="pf-webrow" key={r.id}>
            <input
              className="pf-input pf-web-kw"
              type="text"
              autoComplete="off"
              spellCheck={false}
              placeholder="keyword"
              value={r.kw}
              onChange={(e) => patchRow(r.id, { kw: e.target.value })}
            />
            <input
              className="pf-input pf-web-url"
              type="text"
              autoComplete="off"
              spellCheck={false}
              placeholder="https://…"
              value={r.url}
              onChange={(e) => patchRow(r.id, { url: e.target.value })}
            />
            <button
              type="button"
              className="pf-btn pf-btn-mini"
              aria-label={'Remove row ' + (r.id)}
              onClick={() => {
                setRows((rs) => rs.filter((x) => x.id !== r.id))
                pushWeb()
              }}
            >
              −
            </button>
          </div>
        ))}
        <div className="pf-note">
          <button
            id="pf-websearch-add"
            type="button"
            className="pf-btn pf-btn-mini"
            aria-label="Add web search row"
            onClick={() => {
              const id = nextId.current++
              setRows((rs) => [...rs, { id: id, kw: '', url: '' }])
            }}
          >
            +
          </button>
        </div>
      </div>
    </div>
  )
}
