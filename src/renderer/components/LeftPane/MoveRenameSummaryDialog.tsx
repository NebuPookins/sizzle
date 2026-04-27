import { useEffect } from 'react'

interface Props {
  changes: string[]
  error?: string
  onClose(): void
}

export default function MoveRenameSummaryDialog({ changes, error, onClose }: Props) {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === 'Enter') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0, 0, 0, 0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 3000,
      }}
      onClick={onClose}
    >
      <div
        style={{
          background: 'var(--bg-panel)',
          border: '1px solid var(--border)',
          borderRadius: 10,
          padding: '24px 28px',
          width: 'min(600px, 92vw)',
          boxShadow: '0 16px 48px rgba(0, 0, 0, 0.5)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 4, color: 'var(--text-primary)' }}>
          {error ? 'Move/Rename Failed' : 'Move/Rename Complete'}
        </div>
        {error && (
          <div style={{
            fontSize: 13,
            color: '#f87171',
            marginBottom: 16,
            padding: '8px 10px',
            background: 'rgba(248, 113, 113, 0.1)',
            borderRadius: 4,
            fontFamily: 'monospace',
          }}>
            {error}
          </div>
        )}
        {changes.length > 0 && (
          <>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 10 }}>
              {error ? 'Changes completed before the error:' : 'External changes made:'}
            </div>
            <div style={{
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              maxHeight: 300,
              overflowY: 'auto',
              marginBottom: 20,
            }}>
              {changes.map((change, i) => (
                <div
                  key={i}
                  style={{
                    fontSize: 12,
                    fontFamily: 'monospace',
                    background: 'var(--bg-main)',
                    border: '1px solid var(--border)',
                    borderRadius: 4,
                    padding: '8px 10px',
                    color: 'var(--text-primary)',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-all',
                  }}
                >
                  {change}
                </div>
              ))}
            </div>
          </>
        )}
        {changes.length === 0 && !error && (
          <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 20 }}>
            No external changes were needed.
          </div>
        )}
        <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <button
            onClick={onClose}
            style={{
              background: 'var(--bg-selected)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              borderRadius: 6,
              fontSize: 13,
              padding: '7px 18px',
              cursor: 'pointer',
            }}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  )
}
