/**
 * Pure display overlay: the host triggers it via window.rfLargeType(text, px)
 * (or window.__rfApplyLargeType(text)); Esc or rfLargeTypeOff() hides it.
 */
export function LargeTypeOverlay({ text, size }: { text: string | null; size: number }) {
  if (text == null) return null
  return (
    <div className="rf-lt" role="dialog" aria-label="Large type">
      <div className="rf-lt-text" style={{ fontSize: size + 'px' }}>
        {text}
      </div>
    </div>
  )
}
