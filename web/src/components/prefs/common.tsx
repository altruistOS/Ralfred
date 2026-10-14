// Shared building blocks for the preferences panes. All controls are plain
// native form elements so keyboard nav (tab order, space/enter) behaves like a
// native macOS form.

import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import type { SettingValue } from '../../state'

export interface PaneProps {
  /** live settings values (GET /settings); panes are only mounted once loaded */
  values: Record<string, SettingValue>
  /** optimistic local set + PATCH /settings {key: value} */
  set: (key: string, value: SettingValue) => void
  onToast: (msg: string) => void
}

export function toStr(v: SettingValue | undefined | null): string {
  if (v == null) return ''
  if (Array.isArray(v)) return v.join('\n')
  if (typeof v === 'object') return Object.keys(v).map((k) => k + ' = ' + v[k]).join('\n')
  return String(v)
}

export function toBool(v: SettingValue | undefined | null, def: boolean): boolean {
  if (typeof v === 'boolean') return v
  if (typeof v === 'string') return v === 'true' || v === '1'
  if (typeof v === 'number') return v !== 0
  return def
}

export function toNum(v: SettingValue | undefined | null, def: number): number {
  if (typeof v === 'number' && Number.isFinite(v)) return v
  if (typeof v === 'string' && /^-?\d+(\.\d+)?$/.test(v.trim())) return Number(v)
  return def
}

export function toLines(v: SettingValue | undefined | null): string[] {
  if (Array.isArray(v)) return v.map((s) => String(s))
  if (typeof v === 'string') return v.split('\n')
  return []
}

export function toMap(v: SettingValue | undefined | null): Record<string, string> {
  const out: Record<string, string> = {}
  if (v && typeof v === 'object' && !Array.isArray(v)) {
    for (const k of Object.keys(v)) out[k] = String(v[k])
  }
  return out
}

/** Label + control form row; control is right-aligned per the layout spec. */
export function FRow(props: {
  htmlFor: string
  label: string
  hint?: string
  children: ReactNode
}) {
  return (
    <div className="pf-frow">
      <label htmlFor={props.htmlFor}>
        {props.label}
        {props.hint ? <span className="pf-hint-txt">{props.hint}</span> : null}
      </label>
      <div className="pf-fctl">{props.children}</div>
    </div>
  )
}

export function CheckRow(props: {
  id: string
  label: string
  hint?: string
  checked: boolean
  onChange: (v: boolean) => void
}) {
  return (
    <FRow htmlFor={props.id} label={props.label} hint={props.hint}>
      <input
        id={props.id}
        className="pf-check"
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
    </FRow>
  )
}

/** Integer input with clamp on blur; commits via onCommit only when changed. */
export function NumberField(props: {
  id: string
  value: number
  min: number
  max: number
  onCommit: (n: number) => void
}) {
  const [text, setText] = useState(() => String(props.value))
  const edited = useRef(false)
  useEffect(() => {
    if (!edited.current) setText(String(props.value))
  }, [props.value])
  const commit = (): void => {
    const n = parseInt(text, 10)
    const cur = Number.isFinite(n) ? Math.max(props.min, Math.min(props.max, n)) : props.value
    edited.current = false
    setText(String(cur))
    if (cur !== props.value) props.onCommit(cur)
  }
  return (
    <input
      id={props.id}
      className="pf-input pf-num"
      type="text"
      inputMode="numeric"
      autoComplete="off"
      spellCheck={false}
      value={text}
      onChange={(e) => {
        edited.current = true
        setText(e.target.value)
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur()
      }}
    />
  )
}
