import { useState, useEffect, useCallback, useRef } from 'react'
import type { GitStatus, GitFileChange } from '../../../preload'

interface Props {
  projectPath: string
}

const STATUS_LABEL: Record<string, string> = {
  M: 'M', A: 'A', D: 'D', R: 'R', C: 'C', U: 'U', T: 'T',
}

const STATUS_COLOR: Record<string, string> = {
  M: 'var(--amber)',
  A: 'var(--green)',
  D: 'var(--red)',
  R: '#7ec8e3',
  C: '#7ec8e3',
  U: 'var(--red)',
}

function FileRow({ change }: { change: GitFileChange }) {
  const color = STATUS_COLOR[change.status] ?? 'var(--text-secondary)'
  const label = STATUS_LABEL[change.status] ?? change.status
  const filename = change.path.split('/').pop() ?? change.path
  const dir = change.path.includes('/') ? change.path.slice(0, change.path.lastIndexOf('/') + 1) : ''

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 6,
      padding: '2px 12px',
      fontSize: 11,
      minWidth: 0,
    }}>
      <span style={{
        color,
        fontWeight: 700,
        fontFamily: 'monospace',
        width: 12,
        flexShrink: 0,
        fontSize: 11,
      }}>
        {label}
      </span>
      <span style={{
        color: 'var(--text-primary)',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        minWidth: 0,
      }} title={change.path}>
        {dir && <span style={{ color: 'var(--text-muted)' }}>{dir}</span>}
        {filename}
      </span>
    </div>
  )
}

function UntrackedRow({ filePath }: { filePath: string }) {
  const filename = filePath.split('/').pop() ?? filePath
  const dir = filePath.includes('/') ? filePath.slice(0, filePath.lastIndexOf('/') + 1) : ''
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 6,
      padding: '2px 12px',
      fontSize: 11,
      minWidth: 0,
    }}>
      <span style={{
        color: 'var(--text-muted)',
        fontWeight: 700,
        fontFamily: 'monospace',
        width: 12,
        flexShrink: 0,
        fontSize: 11,
      }}>?</span>
      <span style={{
        color: 'var(--text-secondary)',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        minWidth: 0,
      }} title={filePath}>
        {dir && <span style={{ color: 'var(--text-muted)' }}>{dir}</span>}
        {filename}
      </span>
    </div>
  )
}

function Section({
  title,
  count,
  color,
  children,
  defaultOpen = true,
}: {
  title: string
  count: number
  color: string
  children: React.ReactNode
  defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)

  if (count === 0) return null

  return (
    <div style={{ marginBottom: 4 }}>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          width: '100%',
          background: 'none',
          border: 'none',
          padding: '4px 12px',
          cursor: 'pointer',
          color: 'var(--text-secondary)',
          fontSize: 11,
          textAlign: 'left',
          userSelect: 'none',
        }}
      >
        <span style={{
          color: 'var(--text-muted)',
          fontSize: 9,
          lineHeight: 1,
          flexShrink: 0,
        }}>
          {open ? '▾' : '▸'}
        </span>
        <span style={{ color, fontWeight: 600, fontSize: 11 }}>{title}</span>
        <span style={{
          marginLeft: 'auto',
          background: 'var(--bg-hover)',
          color: 'var(--text-muted)',
          borderRadius: 8,
          padding: '0 5px',
          fontSize: 10,
          fontVariantNumeric: 'tabular-nums',
        }}>
          {count}
        </span>
      </button>
      {open && <div>{children}</div>}
    </div>
  )
}

const POLL_INTERVAL_MS = 5000

export default function GitStatusPane({ projectPath }: Props) {
  const [isGitRepo, setIsGitRepo] = useState<boolean | null>(null)
  const [status, setStatus] = useState<GitStatus | null>(null)
  const [lastRefresh, setLastRefresh] = useState(0)
  const fetchingRef = useRef(false)

  const refresh = useCallback(async () => {
    if (fetchingRef.current) return
    fetchingRef.current = true
    try {
      const result = await window.sizzle.getGitStatus(projectPath)
      setStatus(result)
      setIsGitRepo(result !== null)
    } finally {
      fetchingRef.current = false
      setLastRefresh(Date.now())
    }
  }, [projectPath])

  // Initial load + project change
  useEffect(() => {
    setIsGitRepo(null)
    setStatus(null)
    void refresh()
  }, [projectPath, refresh])

  // Polling
  useEffect(() => {
    const id = window.setInterval(() => void refresh(), POLL_INTERVAL_MS)
    return () => window.clearInterval(id)
  }, [refresh])

  if (isGitRepo === false) return null
  if (isGitRepo === null) return null  // still loading — don't flash

  if (!status) return null

  const totalChanges =
    status.staged.length + status.unstaged.length + status.untracked.length

  return (
    <div style={{
      width: 240,
      minWidth: 240,
      height: '100%',
      background: 'var(--bg-panel)',
      borderLeft: '1px solid var(--border)',
      display: 'flex',
      flexDirection: 'column',
      overflow: 'hidden',
      flexShrink: 0,
    }}>
      {/* Header */}
      <div style={{
        padding: '10px 12px 8px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
        flexShrink: 0,
      }}>
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
            Git
          </span>
          <button
            onClick={() => void refresh()}
            title="Refresh"
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              color: 'var(--text-muted)',
              padding: '2px 4px',
              borderRadius: 4,
              fontSize: 11,
              lineHeight: 1,
            }}
            onMouseOver={(e) => { e.currentTarget.style.color = 'var(--text-secondary)' }}
            onMouseOut={(e) => { e.currentTarget.style.color = 'var(--text-muted)' }}
          >
            ↻
          </button>
        </div>

        {/* Branch */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 5, minWidth: 0 }}>
          <span style={{ color: 'var(--text-muted)', fontSize: 11, flexShrink: 0 }}>⎇</span>
          <span style={{
            color: status.isDetached ? 'var(--amber)' : 'var(--accent)',
            fontWeight: 600,
            fontSize: 12,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }} title={status.branch ?? 'HEAD detached'}>
            {status.isDetached ? 'HEAD detached' : (status.branch ?? '—')}
          </span>
        </div>

        {/* Remote sync */}
        {status.upstream && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 5, fontSize: 11 }}>
            <span style={{ color: 'var(--text-muted)', flexShrink: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={status.upstream}>
              {status.upstream}
            </span>
            {(status.ahead > 0 || status.behind > 0) && (
              <span style={{ marginLeft: 'auto', display: 'flex', gap: 4, flexShrink: 0 }}>
                {status.ahead > 0 && (
                  <span style={{ color: 'var(--green)', fontWeight: 600 }}>↑{status.ahead}</span>
                )}
                {status.behind > 0 && (
                  <span style={{ color: 'var(--amber)', fontWeight: 600 }}>↓{status.behind}</span>
                )}
              </span>
            )}
            {status.ahead === 0 && status.behind === 0 && (
              <span style={{ marginLeft: 'auto', color: 'var(--green)', fontSize: 10 }}>✓ up to date</span>
            )}
          </div>
        )}
        {!status.upstream && !status.isDetached && (
          <span style={{ fontSize: 10, color: 'var(--text-muted)' }}>no remote</span>
        )}

        {/* Clean indicator */}
        {totalChanges === 0 && (
          <span style={{ fontSize: 11, color: 'var(--green)' }}>✓ clean</span>
        )}
      </div>

      {/* File sections */}
      <div style={{ overflowY: 'auto', flex: 1, paddingTop: 6, paddingBottom: 8 }}>
        <Section title="Staged" count={status.staged.length} color="var(--green)">
          {status.staged.map((f, i) => <FileRow key={i} change={f} />)}
        </Section>

        <Section title="Modified" count={status.unstaged.length} color="var(--amber)">
          {status.unstaged.map((f, i) => <FileRow key={i} change={f} />)}
        </Section>

        <Section title="Untracked" count={status.untracked.length} color="var(--text-secondary)" defaultOpen={false}>
          {status.untracked.map((f, i) => <UntrackedRow key={i} filePath={f} />)}
        </Section>
      </div>
    </div>
  )
}
