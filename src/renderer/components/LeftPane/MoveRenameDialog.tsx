import { useEffect, useRef, useState } from 'react'

interface Props {
  projectPath: string
  onClose(): void
  onConfirm(newPath: string): void
}

export default function MoveRenameDialog({ projectPath, onClose, onConfirm }: Props) {
  const [newPath, setNewPath] = useState(projectPath)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
    inputRef.current?.select()
  }, [])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  const handleSubmit = () => {
    const trimmed = newPath.trim()
    if (trimmed && trimmed !== projectPath) {
      onConfirm(trimmed)
    }
  }

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
          width: 'min(560px, 92vw)',
          boxShadow: '0 16px 48px rgba(0, 0, 0, 0.5)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 16, color: 'var(--text-primary)' }}>
          Move/Rename Project
        </div>
        <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 8 }}>
          New path
        </div>
        <input
          ref={inputRef}
          type="text"
          value={newPath}
          onChange={(e) => setNewPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleSubmit()
          }}
          style={{
            width: '100%',
            boxSizing: 'border-box',
            background: 'var(--bg-input, var(--bg-main))',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text-main)',
            fontSize: 13,
            padding: '7px 10px',
            outline: 'none',
            fontFamily: 'monospace',
            marginBottom: 20,
          }}
        />
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button
            onClick={onClose}
            style={{
              background: 'var(--bg-hover)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              borderRadius: 6,
              fontSize: 13,
              padding: '7px 18px',
              cursor: 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!newPath.trim() || newPath.trim() === projectPath}
            style={{
              background: 'var(--bg-selected)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              borderRadius: 6,
              fontSize: 13,
              padding: '7px 18px',
              cursor: 'pointer',
              opacity: !newPath.trim() || newPath.trim() === projectPath ? 0.5 : 1,
            }}
          >
            OK
          </button>
        </div>
      </div>
    </div>
  )
}
