import { useCallback, useEffect, useRef, useState } from 'react'
import { useAppStore, Project } from './store/appStore'
import LeftPane from './components/LeftPane/LeftPane'
import MainPane from './components/MainPane/MainPane'
import GitStatusPane from './components/GitStatusPane/GitStatusPane'
import type { ProjectTag } from './api'
import { scanProjects, getAllMetadata, consumeReloadSnapshot, setWindowTitle, ptyListSessions, getApiManifest } from './api'
import type { LaunchTarget, ReloadSnapshot } from '../shared/reload'
import { API_MANIFEST } from 'virtual:api-manifest'
import { diffManifests, type ManifestDiff } from './diffManifests'
import ApiMismatchBanner from './ApiMismatchBanner'

const PROJECT_REFRESH_INTERVAL_MS = 10_000

interface ParsedAgentId {
  type: 'agent'
  projectPath: string
  sessionNumber: number
  launchTarget: LaunchTarget
}

interface ParsedShellId {
  type: 'shell'
  projectPath: string
  sessionNumber: number
}

type ParsedPtyId = ParsedAgentId | ParsedShellId

function parsePtyId(id: string): ParsedPtyId | null {
  // Format for agent:  <launchTarget>-<projectPath>-<sessionNumber>
  // Format for shell:  shell-<projectPath>-<sessionNumber>
  // The session number is always the last `-<digits>` segment.
  const lastDash = id.lastIndexOf('-')
  if (lastDash === -1) return null

  const sessionStr = id.slice(lastDash + 1)
  const sessionNumber = parseInt(sessionStr, 10)
  if (isNaN(sessionNumber)) return null

  const prefix = id.slice(0, lastDash)

  if (prefix.startsWith('shell-')) {
    return { type: 'shell', projectPath: prefix.slice(6), sessionNumber }
  }

  // Agent session: first `-` separates launch target from absolute path
  const firstDash = prefix.indexOf('-')
  if (firstDash === -1) return null

  const launchTarget = prefix.slice(0, firstDash) as LaunchTarget

  return {
    type: 'agent',
    projectPath: prefix.slice(firstDash + 1),
    sessionNumber,
    launchTarget,
  }
}

async function restoreBackendSessions(): Promise<void> {
  const activeIds = await ptyListSessions()
  if (activeIds.length === 0) return

  const store = useAppStore.getState()

  // Group sessions by project path
  const projectsMap = new Map<string, {
    launchTarget: LaunchTarget
    agentSession: number
    shellTabs: number[]
  }>()

  for (const id of activeIds) {
    const parsed = parsePtyId(id)
    if (!parsed) continue

    if (parsed.type === 'agent') {
      const entry = projectsMap.get(parsed.projectPath)
      if (entry) {
        entry.launchTarget = parsed.launchTarget
        entry.agentSession = Math.max(entry.agentSession, parsed.sessionNumber)
      } else {
        projectsMap.set(parsed.projectPath, {
          launchTarget: parsed.launchTarget,
          agentSession: parsed.sessionNumber,
          shellTabs: [],
        })
      }
    } else {
      const entry = projectsMap.get(parsed.projectPath)
      if (entry) {
        entry.shellTabs.push(parsed.sessionNumber)
      } else {
        // Only shell sessions exist → this is a shell-only project
        projectsMap.set(parsed.projectPath, {
          launchTarget: 'shell',
          agentSession: 0,
          shellTabs: [parsed.sessionNumber],
        })
      }
    }
  }

  const terminals: ReloadSnapshot['terminals'] = []
  let selectedProjectPath: string | null = null

  for (const [projectPath, info] of projectsMap) {
    const shellTabs = info.shellTabs.sort((a, b) => a - b)
    const maxShell = shellTabs.length > 0 ? shellTabs[shellTabs.length - 1] : -1

    terminals.push({
      projectPath,
      launchTarget: info.launchTarget,
      agentSession: info.agentSession,
      shellSession: shellTabs[0] ?? 0,
      shellTabs: shellTabs.length > 0 ? [...shellTabs] : [0],
      activeShellTab: shellTabs[0] ?? 0,
      nextShellSession: maxShell + 1,
      activeTopTab: 'terminal',
    })

    if (!selectedProjectPath) {
      selectedProjectPath = projectPath
    }
  }

  store.hydrateReloadSnapshot({
    selectedProjectPath,
    terminals,
    timestamp: Date.now(),
  })
  store.setReloadMessage('Terminals reconnected.')
}

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

  const [apiDiff, setApiDiff] = useState<ManifestDiff | null>(null)
  const [apiDiffDismissed, setApiDiffDismissed] = useState(false)

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
      } else {
        // Tauri build: restore any active PTY sessions from the backend
        restoreBackendSessions().catch(() => {})
      }
      void loadProjects()
    })

    // Check API sync
    getApiManifest()
      .then((backend) => {
        const d = diffManifests(API_MANIFEST, backend)
        if (d.missing.length > 0 || d.changed.length > 0) {
          setApiDiff(d)
        }
      })
      .catch(() => {
        // Can't reach backend at all — treat as mismatch
        setApiDiff({
          missing: [{ name: '(backend unreachable)', kind: 'missing', frontendArgs: [] }],
          extra: [],
          changed: [],
        })
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
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {apiDiff && !apiDiffDismissed && (
        <ApiMismatchBanner diff={apiDiff} onDismiss={() => setApiDiffDismissed(true)} />
      )}
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
      <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
        <LeftPane onRefreshProjects={loadProjects} />
        <MainPane />
        {selectedProject && <GitStatusPane projectPath={selectedProject.path} />}
      </div>
    </div>
  )
}
