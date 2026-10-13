import type { SearchResult } from '../state'

interface Props {
  id: string
  items: SearchResult[]
  sel: number
  onSel: (i: number) => void
  onExec: (i: number) => void
}

/**
 * Results list. Rows carry the legacy `i` class because the host's selftest
 * probes rows via document.querySelectorAll('.i'). Rows are 46px tall with
 * 28px icons; the hint column shows ⏎ for row 0 and ⌘(n+1) for rows 1..9
 * (⌘1 opens row 0 — the same mapping the keyboard handler uses).
 */
export function Results({ id, items, sel, onSel, onExec }: Props) {
  return (
    <ul id={id} className="rf-list" role="listbox" aria-label="Search results">
      {items.map((it, i) => (
        <li
          key={i + '|' + it.k + '|' + it.t}
          className={'rf-row i' + (i === sel ? ' sel' : '')}
          role="option"
          aria-selected={i === sel}
          onMouseEnter={() => onSel(i)}
          onClick={() => onExec(i)}
        >
          {it.i ? (
            <img
              className="rf-ico"
              src={it.i}
              alt=""
              draggable={false}
              onError={(e) => {
                e.currentTarget.style.visibility = 'hidden'
              }}
            />
          ) : (
            <div className="rf-ico rf-ico-blank" aria-hidden="true" />
          )}
          <div className="rf-mid">
            <div className="rf-title">{it.t}</div>
            {it.s ? <div className="rf-sub">{it.s}</div> : null}
          </div>
          <span className="rf-hint">{i === 0 ? '\u23CE' : i <= 8 ? '\u2318' + (i + 1) : ''}</span>
        </li>
      ))}
    </ul>
  )
}
