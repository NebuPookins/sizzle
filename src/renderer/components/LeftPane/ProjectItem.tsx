import { Project, useAppStore } from '../../store/appStore'
import type { ProjectMarker } from '../../../preload'

interface Props {
  project: Project
  isSelected: boolean
  isLaunched: boolean
  onMarkerChange(project: Project, marker: ProjectMarker): void
  onContextMenuRequest(project: Project, x: number, y: number): void
}

function formatDate(ts: number | null): string {
  if (!ts) return ''
  const d = new Date(ts)
  const now = new Date()
  const diffDays = Math.floor((now.getTime() - d.getTime()) / 86400000)
  if (diffDays === 0) return 'today'
  if (diffDays === 1) return 'yesterday'
  if (diffDays < 7) return `${diffDays}d ago`
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

function nextMarker(marker: ProjectMarker): ProjectMarker {
  if (marker === null) return 'favorite'
  if (marker === 'favorite') return 'ignored'
  return null
}

function MarkerIcon({ marker }: { marker: ProjectMarker }) {
  if (marker === 'favorite') {
    return (
      <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 3.8 14.53 8.93l5.66.82-4.1 4 1 5.64L12 16.73l-5.09 2.66 1-5.64-4.1-4 5.66-.82Z"
          fill="currentColor"
        />
      </svg>
    )
  }

  if (marker === 'ignored') {
    return (
      <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M9 3h6l1 2h4v2H4V5h4Zm1 7h2v8h-2Zm4 0h2v8h-2ZM7 10h2v8H7Zm-1 10h12l1-11H5Z"
          fill="currentColor"
        />
      </svg>
    )
  }

  return (
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M12 3.8 14.53 8.93l5.66.82-4.1 4 1 5.64L12 16.73l-5.09 2.66 1-5.64-4.1-4 5.66-.82Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export default function ProjectItem({ project, isSelected, isLaunched, onMarkerChange, onContextMenuRequest }: Props) {
  const { selectProject, claudeStatus } = useAppStore()
  const status = isLaunched ? (claudeStatus[project.path] ?? 'waiting') : null
  const markerColor = project.marker === 'favorite'
    ? '#f5c451'
    : project.marker === 'ignored'
      ? 'var(--red)'
      : 'var(--text-muted)'

  return (
    <div
      onClick={() => selectProject(project)}
      onContextMenu={(event) => {
        event.preventDefault()
        onContextMenuRequest(project, event.clientX, event.clientY)
      }}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '8px 12px',
        cursor: 'pointer',
        borderRadius: 6,
        background: isSelected ? 'var(--bg-selected)' : 'transparent',
        borderLeft: isSelected ? '3px solid var(--accent)' : '3px solid transparent',
        transition: 'background 0.12s',
      }}
      onMouseEnter={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = 'var(--bg-hover)'
      }}
      onMouseLeave={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = 'transparent'
      }}
    >
      {status && <span className={`status-dot ${status}`} />}
      {!status && <span style={{ width: 8 }} />}

      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <div style={{
            flex: 1,
            minWidth: 0,
            fontSize: 13,
            fontWeight: isLaunched ? 600 : 400,
            color: isSelected ? 'var(--text-primary)' : 'var(--text-primary)',
            whiteSpace: 'nowrap',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            opacity: project.marker === 'ignored' ? 0.72 : 1,
          }}>
            {project.name}
          </div>
          <button
            type="button"
            aria-label={`Cycle project marker for ${project.name}`}
            title={project.marker === null ? 'Neutral' : project.marker === 'favorite' ? 'Favorite' : 'Ignored'}
            onClick={(event) => {
              event.stopPropagation()
              onMarkerChange(project, nextMarker(project.marker))
            }}
            onMouseDown={(event) => event.stopPropagation()}
            style={{
              width: 22,
              height: 22,
              display: 'grid',
              placeItems: 'center',
              background: 'transparent',
              border: 'none',
              borderRadius: 4,
              color: markerColor,
              cursor: 'pointer',
              flexShrink: 0,
            }}
          >
            <MarkerIcon marker={project.marker} />
          </button>
        </div>
        {(project.lastLaunched || project.primaryTag) && (
          <div style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 6,
            marginTop: 2,
          }}>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', flexShrink: 0 }}>
              {formatDate(project.lastLaunched)}
            </div>
            {project.primaryTag && (
              <div style={{
                fontSize: 10,
                color: 'var(--accent)',
                background: 'color-mix(in srgb, var(--accent) 14%, transparent)',
                borderRadius: 3,
                padding: '1px 5px',
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                minWidth: 0,
              }}>
                {project.primaryTag}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
