import { useEffect, useMemo, useState } from 'react'
import * as bridge from '../../bridge'
import type { ThemeInfo } from '../../bridge'
import type { PaneProps } from './common'
import { CheckRow, NumberField, toBool, toNum, toStr } from './common'

const DEFAULTS: Record<string, string> = {
  BG: '1e1e1e',
  FIELD: '111010',
  TEXT: 'f2f2f2',
  SUBTEXT: '8e8e93',
  SELBG: '3a3a3c',
}

function hex(raw: string | undefined, def: string): string {
  const s = (raw == null ? '' : String(raw)).trim().replace(/^#+/, '')
  return '#' + (s.length > 0 ? s : def)
}

/**
 * Palette for the live panel preview: prefer the theme's own `colors` from
 * /api/themes when the host carries them, else the running window theme.
 */
function paletteOf(theme: ThemeInfo | undefined): Record<string, string> {
  const w = (typeof window !== 'undefined' && window.__RF_THEME__) || {}
  const out: Record<string, string> = {}
  for (const k of Object.keys(DEFAULTS)) {
    const fromTheme = theme && theme.colors ? theme.colors[k] : undefined
    const fromWindow = (w as Record<string, string | undefined>)[k]
    out[k] = hex(fromTheme != null ? fromTheme : fromWindow, DEFAULTS[k])
  }
  return out
}

/**
 * Appearance pane: master theme list (GET /api/themes, select PATCHes
 * /settings {theme:id}) + right column with a live 360x48 panel preview and
 * the panel-behaviour controls.
 */
export function AppearancePane({ values, set }: PaneProps) {
  const [themes, setThemes] = useState<ThemeInfo[] | null>(null)
  const [sel, setSel] = useState<string>(() => toStr(values.theme))

  useEffect(() => {
    bridge.themesGet().then(
      (ts) => {
        setThemes(ts)
        setSel((cur) => {
          const want = toStr(values.theme)
          if (want && ts.some((t) => t.id === want)) return want
          return ts.length > 0 ? ts[0].id : ''
        })
      },
      () => setThemes([]),
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const selTheme = useMemo(() => {
    if (!themes) return undefined
    return themes.find((t) => t.id === sel) || themes[0]
  }, [themes, sel])

  const pal = paletteOf(selTheme)

  const pick = (id: string): void => {
    setSel(id)
    if (id !== toStr(values.theme)) set('theme', id)
  }

  return (
    <div className="pf-split">
      <div
        className="pf-master pf-themes"
        id="pf-themes-list"
        role="listbox"
        aria-label="Themes"
        tabIndex={0}
      >
        {themes == null ? (
          <div className="pf-empty-txt">Loading…</div>
        ) : themes.length === 0 ? (
          <div className="pf-empty-txt">No themes available.</div>
        ) : (
          themes.map((t) => (
            <div
              key={t.id}
              className={'pf-mrow' + (t.id === (selTheme ? selTheme.id : sel) ? ' sel' : '')}
              role="option"
              aria-selected={t.id === sel}
              tabIndex={-1}
              onClick={() => pick(t.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') pick(t.id)
              }}
            >
              <span className="nm">{t.name}</span>
            </div>
          ))
        )}
      </div>

      <div className="pf-detail pf-apr-right">
        <div className="pf-title">Preview</div>
        <div className="pf-preview" style={{ background: pal.BG, color: pal.TEXT }} aria-hidden="true">
          <div className="pf-preview-field" style={{ background: pal.FIELD }}>
            <span className="pf-caret" style={{ background: pal.TEXT }} />
          </div>
          <div className="pf-preview-row">
            <span className="pf-preview-ico" />
            <span style={{ color: pal.TEXT }}>Sample Result</span>
            <span style={{ color: pal.SUBTEXT, marginLeft: 6, fontSize: 11 }}>subtitle</span>
          </div>
        </div>

        <div className="pf-apr-controls">
          <CheckRow
            id="pf-translucent"
            label="Translucency (frosted glass)"
            hint="macOS vibrancy blur behind the panel, like Alfred's translucency."
            checked={toBool(values.translucent, true)}
            onChange={(v) => set('translucent', v)}
          />
          <CheckRow
            id="pf-autoexpand"
            label="Auto-expand snippets"
            hint="Expand snippet keyword matches into the results list."
            checked={toBool(values.auto_expand_snippets, true)}
            onChange={(v) => set('auto_expand_snippets', v)}
          />
          <div className="pf-frow">
            <label htmlFor="pf-maxresults">
              Max results
              <span className="pf-hint-txt">Between 5 and 50.</span>
            </label>
            <div className="pf-fctl">
              <NumberField
                id="pf-maxresults"
                value={toNum(values.max_results, 20)}
                min={5}
                max={50}
                onCommit={(n) => set('max_results', n)}
              />
            </div>
          </div>
          <div className="pf-frow">
            <label htmlFor="pf-clipboard">
              Clipboard history items
              <span className="pf-hint-txt">Between 50 and 1000.</span>
            </label>
            <div className="pf-fctl">
              <NumberField
                id="pf-clipboard"
                value={toNum(values.clipboard_max_items, 200)}
                min={50}
                max={1000}
                onCommit={(n) => set('clipboard_max_items', n)}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
