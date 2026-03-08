import { Project, useAppStore } from '../../store/appStore'

interface Props {
  project: Project
  isSelected: boolean
  isLaunched: boolean
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

export default function ProjectItem({ project, isSelected, isLaunched, onContextMenuRequest }: Props) {
  const { selectProject, claudeStatus } = useAppStore()
  const status = isLaunched ? (claudeStatus[project.path] ?? 'waiting') : null

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
        <div style={{
          fontSize: 13,
          fontWeight: isLaunched ? 600 : 400,
          color: isSelected ? 'var(--text-primary)' : 'var(--text-primary)',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
        }}>
          {project.name}
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
