import { useEffect, useState } from 'react'
import { Project, useAppStore } from '../../store/appStore'
import type { ProjectMarker } from '../../api'
import { addIgnoreRoot, setProjectMarker as persistProjectMarker, moveRenameProject, reloadCore } from '../../api'
import type { MoveRenameResult } from '../../api'
import ProjectItem from './ProjectItem'
import ScanSettingsDialog from './ScanSettingsDialog'
import AgentPresetsDialog from './AgentPresetsDialog'
import MoveRenameDialog from './MoveRenameDialog'
import MoveRenameSummaryDialog from './MoveRenameSummaryDialog'

interface Props {
  onRefreshProjects(): Promise<void>
}

interface ContextMenuState {
  project: Project
  x: number
  y: number
}

export default function LeftPane({ onRefreshProjects }: Props) {
  const {
    selectedProject,
    launchedProjects,
    sortedProjects,
    setProjectMarker,
    createReloadSnapshot,
    setReloadMessage,
    autoSwitchMode,
    setAutoSwitchMode,
  } = useAppStore()
  const [search, setSearch] = useState('')
  const [showSettings, setShowSettings] = useState(false)
  const [showAgentPresets, setShowAgentPresets] = useState(false)
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null)
  const [renameTarget, setRenameTarget] = useState<Project | null>(null)
  const [moveRenameSummary, setMoveRenameSummary] = useState<{ changes: string[]; error?: string } | null>(null)
  const allProjects = sortedProjects()
  const projects = search
    ? allProjects.filter(p => {
        const q = search.toLowerCase()
        return p.name.toLowerCase().includes(q) || p.tags.some(t => t.name.toLowerCase().includes(q))
      })
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

  const handleMoveRenameConfirm = async (newPath: string) => {
    if (!renameTarget) return
    const oldPath = renameTarget.path
    setRenameTarget(null)
    const result = await moveRenameProject(oldPath, newPath)
    setMoveRenameSummary({ changes: result.changes, error: result.error })
    if (result.success) {
      await onRefreshProjects()
    }
  }

  const addProjectToIgnoreRoots = async () => {
    if (!contextMenu) return
    await addIgnoreRoot(contextMenu.project.path)
    await onRefreshProjects()
    setContextMenu(null)
  }

  const handleMarkerChange = async (project: Project, marker: ProjectMarker) => {
    const previousMarker = project.marker
    setProjectMarker(project.path, marker)

    try {
      const persistMarker = persistProjectMarker
      if (typeof persistMarker !== 'function') {
        throw new Error('setProjectMarker bridge unavailable')
      }
      const updated = await persistMarker(project.path, marker)
      setProjectMarker(project.path, updated.marker)
    } catch (error) {
      console.error('Failed to persist project marker', error)
      setProjectMarker(project.path, previousMarker)
    }
  }

  const handleReloadCore = async () => {
    const confirmed = window.confirm('Restart the app core and reconnect open terminals?')
    if (!confirmed) return
    try {
      await reloadCore(createReloadSnapshot())
    } catch (error) {
      console.error('Failed to reload core', error)
      setReloadMessage('Core reload failed. The current app is still running.')
    }
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
            onMarkerChange={handleMarkerChange}
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
            {search ? 'No matches' : 'No projects yet. Choose scan settings to add a root folder.'}
          </div>
        )}
      </div>

      <div style={{ padding: '8px', borderTop: '1px solid var(--border)', display: 'grid', gap: 8 }}>
        <label style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          fontSize: 11,
          color: 'var(--text-muted)',
          cursor: 'pointer',
        }}>
          <input
            type="checkbox"
            checked={autoSwitchMode}
            onChange={e => setAutoSwitchMode(e.target.checked)}
          />
          Auto-switch to idle projects
        </label>
        <button
          onClick={handleReloadCore}
          style={{
            width: '100%',
            background: 'var(--bg-hover)',
            border: '1px solid var(--border)',
            color: 'var(--text-primary)',
            borderRadius: 6,
            fontSize: 12,
            padding: '7px 10px',
            cursor: 'pointer',
          }}
        >
          Reload Core
        </button>
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
        <button
          onClick={() => setShowAgentPresets(true)}
          style={{
            width: '100%',
            background: 'var(--bg-hover)',
            border: '1px solid var(--border)',
            color: 'var(--text-primary)',
            borderRadius: 6,
            fontSize: 12,
            padding: '7px 10px',
            cursor: 'pointer',
          }}
        >
          Agent presets
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
            onClick={() => {
              setRenameTarget(contextMenu.project)
              setContextMenu(null)
            }}
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
            Move/Rename project
          </button>
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

      <AgentPresetsDialog
        isOpen={showAgentPresets}
        onClose={() => setShowAgentPresets(false)}
      />

      {renameTarget && (
        <MoveRenameDialog
          projectPath={renameTarget.path}
          onClose={() => setRenameTarget(null)}
          onConfirm={handleMoveRenameConfirm}
        />
      )}

      {moveRenameSummary && (
        <MoveRenameSummaryDialog
          changes={moveRenameSummary.changes}
          error={moveRenameSummary.error}
          onClose={() => setMoveRenameSummary(null)}
        />
      )}
    </div>
  )
}
