import { useEffect, useState } from 'react'
import { Project, useAppStore } from '../../store/appStore'
import ProjectItem from './ProjectItem'
import ScanSettingsDialog from './ScanSettingsDialog'

interface Props {
  onRefreshProjects(): Promise<void>
}

interface ContextMenuState {
  project: Project
  x: number
  y: number
}

export default function LeftPane({ onRefreshProjects }: Props) {
  const { selectedProject, launchedProjects, sortedProjects } = useAppStore()
  const [search, setSearch] = useState('')
  const [showSettings, setShowSettings] = useState(false)
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  const allProjects = sortedProjects()
  const projects = search
    ? allProjects.filter(p => p.name.toLowerCase().includes(search.toLowerCase()))
    : allProjects

  useEffect(() => {
    if (!contextMenu) return
    const clear = () => setContextMenu(null)
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') clear()
    }
    window.addEventListener('click', clear)
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('click', clear)
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [contextMenu])

  const addProjectToIgnoreRoots = async () => {
    if (!contextMenu) return
    await window.sizzle.addIgnoreRoot(contextMenu.project.path)
    await onRefreshProjects()
    setContextMenu(null)
  }

  return (
    <div style={{
      width: 240,
      flexShrink: 0,
      background: 'var(--bg-panel)',
      borderRight: '1px solid var(--border)',
      display: 'flex',
      flexDirection: 'column',
      overflow: 'hidden',
    }}>
      <div style={{
        padding: '14px 12px 10px',
        borderBottom: '1px solid var(--border)',
        fontSize: 13,
        fontWeight: 700,
        letterSpacing: '0.08em',
        textTransform: 'uppercase',
        color: 'var(--text-muted)',
      }}>
        Projects ({projects.length})
      </div>

      <div style={{ padding: '6px 8px', borderBottom: '1px solid var(--border)' }}>
        <input
          type="text"
          placeholder="Filter projects…"
          value={search}
          onChange={e => setSearch(e.target.value)}
          style={{
            width: '100%',
            boxSizing: 'border-box',
            background: 'var(--bg-input, var(--bg-main))',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text-main)',
            fontSize: 12,
            padding: '4px 8px',
            outline: 'none',
          }}
        />
      </div>

      <div style={{ flex: 1, overflowY: 'auto', padding: '6px 4px' }}>
        {projects.map((project) => (
          <ProjectItem
            key={project.path}
            project={project}
            isSelected={selectedProject?.path === project.path}
            isLaunched={launchedProjects.has(project.path)}
            onContextMenuRequest={(menuProject, x, y) => setContextMenu({ project: menuProject, x, y })}
          />
        ))}
        {projects.length === 0 && (
          <div style={{
            padding: '20px 12px',
            color: 'var(--text-muted)',
            fontSize: 12,
            textAlign: 'center',
          }}>
            {search ? 'No matches' : 'Scanning projects…'}
          </div>
        )}
      </div>

      <div style={{ padding: '8px', borderTop: '1px solid var(--border)' }}>
        <button
          onClick={() => setShowSettings(true)}
          style={{
            width: '100%',
            background: 'var(--bg-selected)',
            border: '1px solid var(--border)',
            color: 'var(--text-primary)',
            borderRadius: 6,
            fontSize: 12,
            padding: '7px 10px',
            cursor: 'pointer',
          }}
        >
          Scan settings
        </button>
      </div>

      {contextMenu && (
        <div
          style={{
            position: 'fixed',
            left: contextMenu.x,
            top: contextMenu.y,
            background: 'var(--bg-panel)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            minWidth: 190,
            padding: 4,
            zIndex: 2200,
            boxShadow: '0 8px 24px rgba(0, 0, 0, 0.35)',
          }}
        >
          <button
            onClick={addProjectToIgnoreRoots}
            style={{
              width: '100%',
              textAlign: 'left',
              padding: '8px 10px',
              background: 'transparent',
              border: 'none',
              color: 'var(--text-primary)',
              cursor: 'pointer',
              borderRadius: 4,
            }}
          >
            Add to roots to ignore
          </button>
        </div>
      )}

      <ScanSettingsDialog
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        onSaved={onRefreshProjects}
      />
    </div>
  )
}
