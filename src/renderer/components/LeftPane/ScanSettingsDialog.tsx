import { useEffect, useState } from 'react'
import type { ScanSettings } from '../../../preload/index'

interface Props {
  isOpen: boolean
  onClose(): void
  onSaved(): Promise<void>
}

function normalizeInputPath(value: string): string {
  return value.trim()
}

function uniqPaths(paths: string[]): string[] {
  return Array.from(new Set(paths))
}

export default function ScanSettingsDialog({ isOpen, onClose, onSaved }: Props) {
  const [scanRoots, setScanRoots] = useState<string[]>([])
  const [ignoreRoots, setIgnoreRoots] = useState<string[]>([])
  const [manualProjectRoots, setManualProjectRoots] = useState<string[]>([])
  const [newScanRoot, setNewScanRoot] = useState('')
  const [newIgnoreRoot, setNewIgnoreRoot] = useState('')
  const [newManualProjectRoot, setNewManualProjectRoot] = useState('')
  const [saving, setSaving] = useState(false)
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!isOpen) return
    setLoading(true)
    window.sizzle.getScanSettings()
      .then((settings: ScanSettings) => {
        setScanRoots(settings.scanRoots)
        setIgnoreRoots(settings.ignoreRoots)
        setManualProjectRoots(settings.manualProjectRoots)
      })
      .finally(() => setLoading(false))
  }, [isOpen])

  if (!isOpen) return null

  const addScanRoot = () => {
    const normalized = normalizeInputPath(newScanRoot)
    if (!normalized) return
    setScanRoots((value) => uniqPaths([...value, normalized]))
    setNewScanRoot('')
  }

  const addIgnoreRoot = () => {
    const normalized = normalizeInputPath(newIgnoreRoot)
    if (!normalized) return
    setIgnoreRoots((value) => uniqPaths([...value, normalized]))
    setNewIgnoreRoot('')
  }

  const addManualProjectRoot = () => {
    const normalized = normalizeInputPath(newManualProjectRoot)
    if (!normalized) return
    setManualProjectRoots((value) => uniqPaths([...value, normalized]))
    setNewManualProjectRoot('')
  }

  const browseFor = async (target: 'scan' | 'ignore' | 'manual') => {
    const picked = await window.sizzle.pickDirectory()
    if (!picked) return
    if (target === 'scan') {
      setNewScanRoot(picked)
      return
    }
    if (target === 'manual') {
      setNewManualProjectRoot(picked)
      return
    }
    setNewIgnoreRoot(picked)
  }

  const save = async () => {
    setSaving(true)
    try {
      await window.sizzle.setScanSettings({ scanRoots, ignoreRoots, manualProjectRoots })
      await onSaved()
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
          width: 'min(760px, 92vw)',
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
        <div style={{ fontSize: 16, fontWeight: 700 }}>Scan Settings</div>

        {loading ? (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading settings…</div>
        ) : (
          <>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ fontSize: 13, fontWeight: 600 }}>Roots to scan from</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                {scanRoots.map((root) => (
                  <div
                    key={root}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 8,
                      padding: '6px 8px',
                      borderRadius: 6,
                      background: 'var(--bg-dark)',
                      border: '1px solid var(--border)',
                      fontSize: 12,
                    }}
                  >
                    <span style={{ wordBreak: 'break-all' }}>{root}</span>
                    <button
                      onClick={() => setScanRoots((value) => value.filter((path) => path !== root))}
                      style={{ padding: '3px 8px', cursor: 'pointer' }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  value={newScanRoot}
                  onChange={(event) => setNewScanRoot(event.target.value)}
                  placeholder="Add path to scan"
                  style={{
                    flex: 1,
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    background: 'var(--bg-dark)',
                    color: 'var(--text-primary)',
                    padding: '7px 8px',
                    fontSize: 12,
                  }}
                />
                <button onClick={() => browseFor('scan')} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Browse
                </button>
                <button onClick={addScanRoot} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Add
                </button>
              </div>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ fontSize: 13, fontWeight: 600 }}>Roots to ignore</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                {ignoreRoots.map((root) => (
                  <div
                    key={root}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 8,
                      padding: '6px 8px',
                      borderRadius: 6,
                      background: 'var(--bg-dark)',
                      border: '1px solid var(--border)',
                      fontSize: 12,
                    }}
                  >
                    <span style={{ wordBreak: 'break-all' }}>{root}</span>
                    <button
                      onClick={() => setIgnoreRoots((value) => value.filter((path) => path !== root))}
                      style={{ padding: '3px 8px', cursor: 'pointer' }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  value={newIgnoreRoot}
                  onChange={(event) => setNewIgnoreRoot(event.target.value)}
                  placeholder="Add path to ignore"
                  style={{
                    flex: 1,
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    background: 'var(--bg-dark)',
                    color: 'var(--text-primary)',
                    padding: '7px 8px',
                    fontSize: 12,
                  }}
                />
                <button onClick={() => browseFor('ignore')} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Browse
                </button>
                <button onClick={addIgnoreRoot} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Add
                </button>
              </div>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ fontSize: 13, fontWeight: 600 }}>Manual project roots</div>
              <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                Manually marked roots take precedence. Detected projects inside these folders will be treated as
                subfolders instead.
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                {manualProjectRoots.map((root) => (
                  <div
                    key={root}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 8,
                      padding: '6px 8px',
                      borderRadius: 6,
                      background: 'var(--bg-dark)',
                      border: '1px solid var(--border)',
                      fontSize: 12,
                    }}
                  >
                    <span style={{ wordBreak: 'break-all' }}>{root}</span>
                    <button
                      onClick={() => setManualProjectRoots((value) => value.filter((path) => path !== root))}
                      style={{ padding: '3px 8px', cursor: 'pointer' }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  value={newManualProjectRoot}
                  onChange={(event) => setNewManualProjectRoot(event.target.value)}
                  placeholder="Add manual project root"
                  style={{
                    flex: 1,
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    background: 'var(--bg-dark)',
                    color: 'var(--text-primary)',
                    padding: '7px 8px',
                    fontSize: 12,
                  }}
                />
                <button onClick={() => browseFor('manual')} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Browse
                </button>
                <button onClick={addManualProjectRoot} style={{ padding: '7px 10px', cursor: 'pointer' }}>
                  Add
                </button>
              </div>
            </div>
          </>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button onClick={onClose} style={{ padding: '7px 12px', cursor: 'pointer' }}>
            Cancel
          </button>
          <button onClick={save} disabled={saving || loading} style={{ padding: '7px 12px', cursor: 'pointer' }}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  )
}
