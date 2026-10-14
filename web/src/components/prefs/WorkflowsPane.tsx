import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import * as bridge from '../../bridge'
import type { WorkflowInfo } from '../../bridge'
import type { PaneProps } from './common'

/**
 * Workflows pane: left column has the filter field (213x22-ish) + the list
 * from GET /api/workflows (name + keyword subtitle); the right column shows
 * the selection's name, keyword and directory (monospace).
 */
export function WorkflowsPane({ onToast }: PaneProps) {
  const [wfs, setWfs] = useState<WorkflowInfo[] | null>(null)
  const [filter, setFilter] = useState('')
  const [sel, setSel] = useState<string | null>(null)

  useEffect(() => {
    bridge.workflowsGet().then(
      (ws) => setWfs(ws),
      () => {
        setWfs([])
        onToast('workflows: could not load')
      },
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const visible = useMemo(() => {
    if (!wfs) return []
    const f = filter.trim().toLowerCase()
    if (!f) return wfs
    return wfs.filter((w) => (w.name + ' ' + w.keyword).toLowerCase().indexOf(f) >= 0)
  }, [wfs, filter])

  const selWf = wfs && sel ? wfs.find((w) => w.uid === sel) : undefined

  const rows: ReactNode[] = []
  visible.forEach((w) => {
    rows.push(
      <div
        key={w.uid}
        className={'pf-mrow pf-wf-row' + (w.uid === sel ? ' sel' : '')}
        role="option"
        aria-selected={w.uid === sel}
        tabIndex={-1}
        onClick={() => setSel(w.uid)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') setSel(w.uid)
        }}
      >
        <div className="nm">{w.name}</div>
        {w.keyword ? <div className="kw">{w.keyword}</div> : null}
      </div>,
    )
  })

  return (
    <div className="pf-split">
      <div className="pf-wf-left">
        <input
          id="pf-wf-search"
          className="pf-input pf-wf-filter"
          type="text"
          autoComplete="off"
          spellCheck={false}
          placeholder="Filter workflows…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="pf-master pf-wfs" id="pf-wf-list" role="listbox" aria-label="Workflows" tabIndex={0}>
          {wfs == null ? (
            <div className="pf-empty-txt">Loading…</div>
          ) : wfs.length === 0 ? (
            <div className="pf-empty-txt">No workflows installed.</div>
          ) : visible.length === 0 ? (
            <div className="pf-empty-txt">No matches.</div>
          ) : (
            rows
          )}
        </div>
      </div>

      {selWf ? (
        <div className="pf-detail">
          <div className="pf-wf-name">{selWf.name}</div>
          <div className="pf-frow">
            <label>Keyword</label>
            <div className="pf-fctl">
              <span className="pf-mono">{selWf.keyword || '—'}</span>
            </div>
          </div>
          <div className="pf-frow pf-frow-col">
            <label>Directory</label>
            <div className="pf-path pf-mono">{selWf.dir}</div>
          </div>
          <div className="pf-note">
            Tip: right-click this result in the panel for 'Show in Finder' to reveal its folder.
          </div>
        </div>
      ) : (
        <div className="pf-detail" />
      )}
    </div>
  )
}
