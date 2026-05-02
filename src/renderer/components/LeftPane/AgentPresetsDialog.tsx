import { useEffect, useState } from 'react'
import { getAgentPresets, setAgentPresets } from '../../api'
import { useAppStore } from '../../store/appStore'
import type { AgentPreset } from '../../api'

interface Props {
  isOpen: boolean
  onClose(): void
}

function emptyPreset(): AgentPreset {
  return { label: '', command: '' }
}

export default function AgentPresetsDialog({ isOpen, onClose }: Props) {
  const [presets, setPresets] = useState<AgentPreset[]>([])
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!isOpen) return
    setLoading(true)
    getAgentPresets()
      .then(setPresets)
      .finally(() => setLoading(false))
  }, [isOpen])

  if (!isOpen) return null

  const updatePreset = (index: number, field: 'label' | 'command', value: string) => {
    setPresets((current) => {
      const next = current.map((p, i) => (i === index ? { ...p, [field]: value } : p))
      return next
    })
  }

  const removePreset = (index: number) => {
    setPresets((current) => current.filter((_, i) => i !== index))
  }

  const addPreset = () => {
    setPresets((current) => [...current, emptyPreset()])
  }

  const save = async () => {
    const valid = presets.filter((p) => p.label.trim() && p.command.trim())
    setSaving(true)
    try {
      await setAgentPresets(valid)
      useAppStore.getState().setAgentPresets(valid)
      onClose()
    } finally {
      setSaving(false)
    }
  }

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0, 0, 0, 0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 2000,
      }}
    >
      <div
        onClick={(event) => event.stopPropagation()}
        style={{
          width: 'min(560px, 92vw)',
          maxHeight: '82vh',
          overflowY: 'auto',
          background: 'var(--bg-panel)',
          border: '1px solid var(--border)',
          borderRadius: 10,
          padding: 16,
          display: 'flex',
          flexDirection: 'column',
          gap: 14,
        }}
      >
        <div style={{ fontSize: 16, fontWeight: 700 }}>Custom Agent Presets</div>

        {loading ? (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading presets…</div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {presets.length === 0 && (
              <div style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                No custom presets yet. Add one below.
              </div>
            )}
            {presets.map((preset, index) => (
              <div
                key={index}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '8px 10px',
                  borderRadius: 6,
                  background: 'var(--bg-dark)',
                  border: '1px solid var(--border)',
                }}
              >
                <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 6 }}>
                  <input
                    value={preset.label}
                    onChange={(event) => updatePreset(index, 'label', event.target.value)}
                    placeholder="Label (e.g. Dev Server)"
                    style={{
                      width: '100%',
                      boxSizing: 'border-box',
                      border: '1px solid var(--border)',
                      borderRadius: 4,
                      background: 'var(--bg-input, var(--bg-main))',
                      color: 'var(--text-primary)',
                      padding: '5px 8px',
                      fontSize: 12,
                    }}
                  />
                  <input
                    value={preset.command}
                    onChange={(event) => updatePreset(index, 'command', event.target.value)}
                    placeholder="Command (e.g. npm run dev)"
                    style={{
                      width: '100%',
                      boxSizing: 'border-box',
                      border: '1px solid var(--border)',
                      borderRadius: 4,
                      background: 'var(--bg-input, var(--bg-main))',
                      color: 'var(--text-primary)',
                      padding: '5px 8px',
                      fontSize: 12,
                      fontFamily: 'monospace',
                    }}
                  />
                </div>
                <button
                  onClick={() => removePreset(index)}
                  style={{
                    padding: '5px 10px',
                    fontSize: 12,
                    cursor: 'pointer',
                    border: '1px solid #5a2020',
                    borderRadius: 4,
                    background: '#2a1010',
                    color: '#c07070',
                    flexShrink: 0,
                  }}
                >
                  Remove
                </button>
              </div>
            ))}
            <div>
              <button
                onClick={addPreset}
                style={{
                  padding: '6px 14px',
                  fontSize: 12,
                  cursor: 'pointer',
                  border: '1px solid var(--border)',
                  borderRadius: 6,
                  background: 'var(--bg-hover)',
                  color: 'var(--text-primary)',
                }}
              >
                + Add preset
              </button>
            </div>
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button onClick={onClose} style={{ padding: '7px 12px', cursor: 'pointer' }}>
            Cancel
          </button>
          <button
            onClick={save}
            disabled={saving || loading}
            style={{ padding: '7px 12px', cursor: 'pointer' }}
          >
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  )
}
