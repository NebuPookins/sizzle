import { useState } from 'react'
import type { ManifestDiff, ManifestDiffEntry } from './diffManifests'

interface Props {
  diff: ManifestDiff
  onDismiss: () => void
}

const sectionStyle: React.CSSProperties = {
  background: 'var(--bg-panel)',
  borderBottom: '1px solid var(--border)',
  fontSize: 13,
  color: 'var(--text-primary)',
  zIndex: 2000,
}

const barStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  padding: '6px 14px',
}

const btnStyle: React.CSSProperties = {
  background: 'transparent',
  border: '1px solid var(--border)',
  borderRadius: 4,
  color: 'var(--text-primary)',
  padding: '2px 10px',
  fontSize: 12,
  cursor: 'pointer',
}

const dismissStyle: React.CSSProperties = {
  ...btnStyle,
  marginLeft: 'auto',
  border: 'none',
  fontSize: 16,
  lineHeight: 1,
  padding: '0 4px',
  opacity: 0.6,
}

const panelStyle: React.CSSProperties = {
  padding: '8px 14px 12px',
  borderTop: '1px solid var(--border)',
  fontSize: 12,
  lineHeight: 1.6,
}

const groupHeader: React.CSSProperties = {
  fontWeight: 600,
  marginBottom: 2,
}

function EntryRow({ entry }: { entry: ManifestDiffEntry }) {
  const icon =
    entry.kind === 'missing' ? '\u{1F534}' : // 🔴
    entry.kind === 'changed' ? '\u{1F7E1}' : // 🟡
    '\u{1F535}' // 🔵

  const desc =
    entry.kind === 'missing'
      ? `Frontend expects "${entry.name}(${(entry.frontendArgs ?? []).join(', ')})" but backend doesn't expose it`
      : entry.kind === 'extra'
        ? `Backend has "${entry.name}(${(entry.backendArgs ?? []).join(', ')})" but frontend doesn't call it`
        : `"${entry.name}" args changed: frontend sends [${(entry.frontendArgs ?? []).join(', ')}] but backend expects [${(entry.backendArgs ?? []).join(', ')}]`

  return (
    <div style={{ paddingLeft: 8, marginBottom: 1 }}>
      {icon} {desc}
    </div>
  )
}

export default function ApiMismatchBanner({ diff, onDismiss }: Props) {
  const [expanded, setExpanded] = useState(false)
  const hasItems = diff.missing.length > 0 || diff.changed.length > 0 || diff.extra.length > 0

  if (!hasItems) return null

  return (
    <div style={sectionStyle}>
      <div style={barStyle}>
        <span style={{ fontSize: 16 }}>{'⚠️'}</span>
        <span style={{ color: 'var(--amber)' }}>API sync mismatch</span>
        <button style={btnStyle} onClick={() => setExpanded(!expanded)}>
          {expanded ? 'Hide diff' : 'View diff'}
        </button>
        <button style={dismissStyle} onClick={onDismiss} title="Dismiss">
          {'✕'}
        </button>
      </div>

      {expanded && (
        <div style={panelStyle}>
          {diff.missing.length > 0 && (
            <div style={{ marginBottom: 8 }}>
              <div style={{ ...groupHeader, color: 'var(--red)' }}>
                {'\u{1F534}'} Missing ({diff.missing.length}) — will cause runtime errors, restart recommended
              </div>
              {diff.missing.map((e) => <EntryRow key={e.name} entry={e} />)}
            </div>
          )}

          {diff.changed.length > 0 && (
            <div style={{ marginBottom: 8 }}>
              <div style={{ ...groupHeader, color: 'var(--amber)' }}>
                {'\u{1F7E1}'} Changed ({diff.changed.length}) — may break, restart recommended
              </div>
              {diff.changed.map((e) => <EntryRow key={e.name} entry={e} />)}
            </div>
          )}

          {diff.extra.length > 0 && (
            <div>
              <div style={{ ...groupHeader, color: 'var(--green)' }}>
                {'\u{1F535}'} New ({diff.extra.length}) — backend added features, safe to ignore
              </div>
              {diff.extra.map((e) => <EntryRow key={e.name} entry={e} />)}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
