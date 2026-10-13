/** Inline toast; auto-dismissed by App after ~2.6s. */
export function Toast({ text }: { text: string | null }) {
  if (!text) return null
  return (
    <div className="rf-toast" role="status">
      {text}
    </div>
  )
}
