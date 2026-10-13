import { useEffect, useRef, useState } from 'react'
import type { Dispatch } from 'react'
import { debounce } from '../state'
import type { Action, SettingsDoc, SettingsField, SettingValue } from '../state'
import * as bridge from '../bridge'

interface Props {
  doc: SettingsDoc
  dispatch: Dispatch<Action>
  onToast: (msg: string) => void
}

/** Renders a settings doc: section sidebar + typed field editors. Edits are
 * stored via SETTINGS_SET and PATCHed to the host (coalesced, error toast). */
export function SettingsOverlay({ doc, dispatch, onToast }: Props) {
  const [section, setSection] = useState(0)
  const sections = doc.schema && doc.schema.sections ? doc.schema.sections : []
  const current = sections[section] || sections[0]

  const docRef = useRef(doc)
  docRef.current = doc
  const dirty = useRef<Set<string>>(new Set())
  const onToastRef = useRef(onToast)
  onToastRef.current = onToast

  const patch = (keys?: string[]): void => {
    const body: Record<string, SettingValue> = {}
    const d = docRef.current
    if (!d) return
    for (const key of keys || Array.from(dirty.current)) {
      dirty.current.delete(key)
      const field = findField(d, key)
      const coerced = coerce(field ? field.type : 'string', d.values[key])
      if (coerced.ok) body[key] = coerced.value
    }
    if (Object.keys(body).length === 0) return
    bridge.settingsPatch(body).then(undefined, (err: unknown) => {
      onToastRef.current('settings: ' + (err instanceof Error ? err.message : 'save failed'))
    })
  }
  const patchRef = useRef(patch)
  patchRef.current = patch

  const debouncedPatch = useRef(debounce(() => { patchRef.current() }, 600)).current

  const commit = (field: SettingsField, value: SettingValue, now: boolean): void => {
    dirty.current.add(field.key)
    dispatch({ type: 'SETTINGS_SET', key: field.key, value })
    if (now) {
      debouncedPatch.cancel()
      patchRef.current([field.key])
    } else {
      debouncedPatch()
    }
  }

  // flush unsaved edits when the overlay closes
  useEffect(() => {
    return () => {
      debouncedPatch.cancel()
      patchRef.current()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="rf-settings" role="dialog" aria-label="Settings">
      <div className="rf-set-nav" role="tablist" aria-label="Sections">
        {sections.map((sx, i) => (
          <button
            key={sx.id || i}
            type="button"
            role="tab"
            aria-selected={i === section}
            className={i === section ? 'sel' : ''}
            onClick={() => setSection(i)}
          >
            {sx.title}
          </button>
        ))}
      </div>

      <div className="rf-set-body">
        {current && current.fields
          ? current.fields.map((f) => (
              <FieldRow
                key={f.key}
                field={f}
                value={doc.values[f.key]}
                commit={(v, now) => commit(f, v, now)}
              />
            ))
          : null}
      </div>

      <div className="rf-set-close">
        <button type="button" className="rf-btn" onClick={() => dispatch({ type: 'SETTINGS_CLOSE' })}>
          Done
        </button>
      </div>
    </div>
  )
}

function FieldRow({
  field,
  value,
  commit,
}: {
  field: SettingsField
  value: SettingValue | undefined
  commit: (value: SettingValue, now: boolean) => void
}) {
  const id = 'f-' + field.key
  const text = str(value)
  let ctl: React.ReactNode

  switch (field.type) {
    case 'bool':
      ctl = (
        <input
          type="checkbox"
          id={id}
          checked={!!value}
          onChange={(e) => commit(e.target.checked, true)}
        />
      )
      break
    case 'select':
      ctl = (
        <select id={id} value={text} onChange={(e) => commit(e.target.value, true)}>
          {(field.options || []).map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )
      break
    case 'int':
      ctl = (
        <input
          id={id}
          type="text"
          value={text}
          onChange={(e) => {
            const v = e.target.value
            commit(/^-?\d+$/.test(v.trim()) ? Number(v) : v, false)
          }}
        />
      )
      break
    case 'list':
      ctl = (
        <textarea
          id={id}
          value={text}
          onChange={(e) => commit(e.target.value.split('\n'), false)}
        />
      )
      break
    case 'map':
      ctl = (
        <textarea
          id={id}
          value={text}
          onChange={(e) => commit(parseMap(e.target.value), false)}
        />
      )
      break
    default:
      ctl = (
        <input
          id={id}
          type="text"
          value={text}
          autoComplete="off"
          spellCheck={false}
          onChange={(e) => commit(e.target.value, false)}
        />
      )
  }

  return (
    <div className="rf-frow">
      <label htmlFor={id}>
        {field.label}
        {field.hint ? <span className="rf-hint-txt">{field.hint}</span> : null}
      </label>
      {ctl}
    </div>
  )
}

function str(v: SettingValue | undefined): string {
  if (v == null) return ''
  if (Array.isArray(v)) return v.join('\n')
  if (typeof v === 'object') return Object.keys(v).map((k) => k + ' = ' + v[k]).join('\n')
  return String(v)
}

function parseMap(text: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const line of text.split('\n')) {
    const i = line.indexOf('=')
    if (i > 0) out[line.slice(0, i).trim()] = line.slice(i + 1).trim()
  }
  return out
}

/** Host PATCH expects typed JSON; map the store's loose value back. */
function coerce(
  type: string,
  v: SettingValue | undefined,
): { ok: boolean; value: SettingValue } {
  switch (type) {
    case 'int':
      if (typeof v === 'number' && Number.isFinite(v)) return { ok: true, value: Math.round(v) }
      if (typeof v === 'string' && /^-?\d+$/.test(v.trim())) return { ok: true, value: Number(v) }
      return { ok: false, value: '' }
    case 'bool':
      return { ok: true, value: !!v }
    case 'list': {
      if (Array.isArray(v)) return { ok: true, value: v.filter((s) => s.length > 0) }
      if (typeof v === 'string')
        return {
          ok: true,
          value: v.split('\n').filter((s) => s.trim().length > 0),
        }
      return { ok: false, value: '' }
    }
    case 'map':
      if (v && typeof v === 'object' && !Array.isArray(v)) {
        return { ok: true, value: v as Record<string, string> }
      }
      return { ok: true, value: {} }
    default:
      return { ok: true, value: typeof v === 'string' ? v : str(v) }
  }
}

function findField(doc: SettingsDoc, key: string): SettingsField | null {
  const sections = (doc.schema && doc.schema.sections) || []
  for (const s of sections) {
    for (const f of s.fields || []) {
      if (f.key === key) return f
    }
  }
  return null
}
