import { useEffect, useState } from 'react'
import * as bridge from '../../bridge'
import type { FeatureInfo } from '../../bridge'
import type { PaneProps } from './common'

/**
 * Features pane: master list from GET /api/features with an enabled dot, and
 * a detail column (64px icon block, title, description, bottom-left Enabled
 * checkbox that PATCHes /api/features {id, enabled}). Nothing renders on the
 * right until something is selected.
 */
export function FeaturesPane({ onToast }: PaneProps) {
  const [feats, setFeats] = useState<FeatureInfo[] | null>(null)
  const [sel, setSel] = useState<string | null>(null)

  useEffect(() => {
    bridge.featuresGet().then(
      (fs) => setFeats(fs),
      () => {
        setFeats([])
        onToast('features: could not load')
      },
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const toggle = (id: string, enabled: boolean): void => {
    setFeats((fs) => (fs ? fs.map((f) => (f.id === id ? { ...f, enabled: enabled } : f)) : fs))
    bridge.featuresPatch(id, enabled).then(undefined, (err: unknown) => {
      setFeats((fs) => (fs ? fs.map((f) => (f.id === id ? { ...f, enabled: !enabled } : f)) : fs))
      onToast('features: ' + (err instanceof Error ? err.message : 'save failed'))
    })
  }

  const selFeat = feats && sel ? feats.find((f) => f.id === sel) : undefined
  const glyph = (f: FeatureInfo): string => {
    const g = String(f.icon || '').trim()
    return g.length > 0 ? g : String(f.name || '?').charAt(0).toUpperCase()
  }

  return (
    <div className="pf-split">
      <div
        className="pf-master pf-features"
        id="pf-features-list"
        role="listbox"
        aria-label="Features"
        tabIndex={0}
      >
        {feats == null ? (
          <div className="pf-empty-txt">Loading…</div>
        ) : feats.length === 0 ? (
          <div className="pf-empty-txt">No features available.</div>
        ) : (
          feats.map((f) => (
            <div
              key={f.id}
              className={'pf-mrow' + (f.id === sel ? ' sel' : '')}
              role="option"
              aria-selected={f.id === sel}
              tabIndex={-1}
              onClick={() => setSel(f.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') setSel(f.id)
              }}
            >
              <span className={'pf-dot' + (f.enabled ? ' on' : '')} aria-hidden="true" />
              <span className="nm">{f.name}</span>
            </div>
          ))
        )}
      </div>

      {selFeat ? (
        <div className="pf-detail">
          <div className="pf-feat-ico">{glyph(selFeat)}</div>
          <div className="pf-feat-title">{selFeat.name}</div>
          <div className="pf-feat-desc">{selFeat.desc}</div>
          <div className="pf-feat-foot">
            <label className="pf-checklabel" htmlFor="pf-fe-enabled">
              <input
                id="pf-fe-enabled"
                className="pf-check"
                type="checkbox"
                checked={!!selFeat.enabled}
                onChange={(e) => toggle(selFeat.id, e.target.checked)}
              />
              Enabled
            </label>
          </div>
        </div>
      ) : (
        <div className="pf-detail" />
      )}
    </div>
  )
}
