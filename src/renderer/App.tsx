import { useCallback, useEffect, useRef } from 'react'
import { useAppStore, Project } from './store/appStore'
import LeftPane from './components/LeftPane/LeftPane'
import MainPane from './components/MainPane/MainPane'
import GitStatusPane from './components/GitStatusPane/GitStatusPane'
import type { ProjectTag } from './api'
import { scanProjects, getAllMetadata, consumeReloadSnapshot, setWindowTitle } from './api'

const PROJECT_REFRESH_INTERVAL_MS = 10_000

function sortTags(tags: ProjectTag[]): ProjectTag[] {
  return [...tags].sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
}

export default function App() {
  const { setProjects, hydrateReloadSnapshot, reloadMessage, setReloadMessage, selectedProject, claudeStatus, shellStatus, autoSwitchMode } = useAppStore()
  const isLoadingProjectsRef = useRef(false)
  const prevClaudeRef = useRef<Record<string, string>>({})
  const prevShellRef = useRef<Record<string, string>>({})
  const lastInteractionRef = useRef(0)
  const autoSwitchEnabledRef = useRef(false)

  autoSwitchEnabledRef.current = autoSwitchMode

  const loadProjects = useCallback(async () => {
    if (isLoadingProjectsRef.current) return
    isLoadingProjectsRef.current = true
    const [scanned, allMeta] = await Promise.all([
      scanProjects(),
      getAllMetadata(),
    ])
      .finally(() => {
        isLoadingProjectsRef.current = false
      })

    const projects: Project[] = scanned.map((p) => {
      const override = allMeta[p.path]?.tagOverride ?? null
      const tags = sortTags(override?.tags ?? p.detectedTags)
      return {
        ...p,
        lastLaunched: allMeta[p.path]?.lastLaunched ?? null,
        tagOverride: override,
        marker: allMeta[p.path]?.marker ?? null,
        tags,
        primaryTag: override?.primaryTag ?? tags[0]?.name ?? null,
      }
    })
    setProjects(projects)
  }, [setProjects])

  useEffect(() => {
    consumeReloadSnapshot().then((snapshot) => {
      if (snapshot) {
        hydrateReloadSnapshot(snapshot)
      }
      void loadProjects()
    })
  }, [hydrateReloadSnapshot, loadProjects])

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void loadProjects()
    }, PROJECT_REFRESH_INTERVAL_MS)

    const refreshOnFocus = () => {
      void loadProjects()
    }
    const refreshOnVisibility = () => {
      if (document.visibilityState === 'visible') {
        void loadProjects()
      }
    }

    window.addEventListener('focus', refreshOnFocus)
    document.addEventListener('visibilitychange', refreshOnVisibility)

    return () => {
      window.clearInterval(intervalId)
      window.removeEventListener('focus', refreshOnFocus)
      document.removeEventListener('visibilitychange', refreshOnVisibility)
    }
  }, [loadProjects])

  useEffect(() => {
    setWindowTitle(selectedProject?.name ?? null)
  }, [selectedProject])

  useEffect(() => {
    if (!reloadMessage) return
    const timer = window.setTimeout(() => setReloadMessage(null), 4000)
    return () => window.clearTimeout(timer)
  }, [reloadMessage, setReloadMessage])

  // Track user interaction to debounce auto-switch; on Enter, switch to an idle project
  useEffect(() => {
    const onInteraction = () => { lastInteractionRef.current = Date.now() }

    const onEnter = (e: KeyboardEvent) => {
      lastInteractionRef.current = Date.now()
      if (e.key !== 'Enter' || !autoSwitchEnabledRef.current) return

      const state = useAppStore.getState()
      const currentPath = state.selectedProject?.path
      if (!currentPath) return

      for (const project of state.projects) {
        if (project.path === currentPath) continue
        if (!state.launchedProjects.has(project.path)) continue

        const isBusy = state.claudeStatus[project.path] === 'working' || state.shellStatus[project.path] === 'working'
        if (isBusy) continue

        state.selectProject(project)
        return
      }
    }

    window.addEventListener('keydown', onEnter)
    window.addEventListener('mousedown', onInteraction)
    return () => {
      window.removeEventListener('keydown', onEnter)
      window.removeEventListener('mousedown', onInteraction)
    }
  }, [])

  // Watch for projects transitioning from busy to idle and auto-switch
  useEffect(() => {
    if (!autoSwitchEnabledRef.current) {
      prevClaudeRef.current = {}
      prevShellRef.current = {}
      return
    }

    const { projects, selectProject } = useAppStore.getState()

    for (const projectPath of Object.keys(claudeStatus)) {
      const isSelected = projectPath === selectedProject?.path
      if (isSelected) continue

      const prevC = prevClaudeRef.current[projectPath]
      const prevS = prevShellRef.current[projectPath]
      const currC = claudeStatus[projectPath]
      const currS = shellStatus[projectPath]

      // Detect transition from busy (any working) to fully idle (all waiting)
      const wasBusy = projectPath in prevClaudeRef.current && (prevC === 'working' || prevS === 'working')
      const nowIdle = currC === 'waiting' && currS === 'waiting'

      if (wasBusy && nowIdle) {
        const selectedClaude = selectedProject?.path ? claudeStatus[selectedProject.path] : undefined
        const selectedShell = selectedProject?.path ? shellStatus[selectedProject.path] : undefined
        const selectedBusy = selectedClaude === 'working' || selectedShell === 'working'

        if (selectedBusy && Date.now() - lastInteractionRef.current > 5000) {
          const project = projects.find(p => p.path === projectPath)
          if (project) {
            selectProject(project)
            break
          }
        }
      }
    }

    prevClaudeRef.current = claudeStatus
    prevShellRef.current = shellStatus
  }, [claudeStatus, shellStatus, selectedProject, autoSwitchMode])

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      {reloadMessage && (
        <div style={{
          position: 'fixed',
          top: 12,
          left: '50%',
          transform: 'translateX(-50%)',
          zIndex: 3000,
          background: 'var(--bg-panel)',
          border: '1px solid var(--border)',
          borderRadius: 8,
          padding: '10px 14px',
          fontSize: 12,
          color: 'var(--text-primary)',
          boxShadow: '0 8px 24px rgba(0, 0, 0, 0.35)',
        }}>
          {reloadMessage}
        </div>
      )}
      <LeftPane onRefreshProjects={loadProjects} />
      <MainPane />
      {selectedProject && <GitStatusPane projectPath={selectedProject.path} />}
    </div>
  )
}
