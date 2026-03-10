import { create } from 'zustand'
import type { ProjectMarker, ProjectTag, ProjectTagOverride } from '../../preload'
import type { LaunchTarget, ReloadSnapshot } from '../../shared/reload'

export interface Project {
  name: string
  path: string
  readmeFiles: string[]
  lastLaunched: number | null
  detectedTags: ProjectTag[]
  tags: ProjectTag[]
  primaryTag: string | null
  tagOverride: ProjectTagOverride | null
  marker: ProjectMarker
}

export interface ProjectTerminalState {
  launchTarget: LaunchTarget
  agentSession: number
  shellTabs: number[]
  activeShellTab: number
  nextShellSession: number
  activeTopTab: 'terminal' | 'explorer' | string
}

type ClaudeStatus = 'working' | 'waiting'
type ShellStatus = 'working' | 'waiting'

interface AppState {
  projects: Project[]
  selectedProjectPath: string | null
  selectedProject: Project | null
  launchedProjects: Set<string>
  terminalStates: Record<string, ProjectTerminalState>
  claudeStatus: Record<string, ClaudeStatus>
  shellStatus: Record<string, ShellStatus>
  reloadMessage: string | null

  setProjects(projects: Project[]): void
  selectProject(project: Project): void
  launchProject(project: Project, target: LaunchTarget): void
  unlaunchProject(path: string): void
  setClaudeStatus(projectPath: string, status: ClaudeStatus): void
  setShellStatus(projectPath: string, status: ShellStatus): void
  setProjectTagOverride(projectPath: string, override: ProjectTagOverride | null): void
  setProjectMarker(projectPath: string, marker: ProjectMarker): void
  setProjectDetectedTags(projectPath: string, detectedTags: ProjectTag[]): void
  setActiveTopTab(projectPath: string, tab: 'terminal' | 'explorer' | string): void
  setActiveShellTab(projectPath: string, shellSession: number): void
  createShellTab(projectPath: string): void
  closeShellTab(projectPath: string, shellSession: number): void
  relaunchAgentTerminal(projectPath: string): void
  relaunchShellTab(projectPath: string, shellSession: number): void
  getTerminalState(projectPath: string): ProjectTerminalState | null
  hydrateReloadSnapshot(snapshot: ReloadSnapshot): void
  createReloadSnapshot(): ReloadSnapshot
  setReloadMessage(message: string | null): void
  sortedProjects(): Project[]
}

function resolveSelectedProject(projects: Project[], selectedProjectPath: string | null): Project | null {
  return selectedProjectPath
    ? projects.find((project) => project.path === selectedProjectPath) ?? null
    : null
}

function defaultTerminalState(launchTarget: LaunchTarget): ProjectTerminalState {
  return {
    launchTarget,
    agentSession: 0,
    shellTabs: [0],
    activeShellTab: 0,
    nextShellSession: 1,
    activeTopTab: 'terminal',
  }
}

function normalizeTerminalState(
  state: Partial<ProjectTerminalState> & { shellSession?: number } | undefined,
  launchTarget: LaunchTarget,
): ProjectTerminalState {
  const fallbackShellSession = state?.shellSession ?? 0
  const shellTabs = state?.shellTabs && state.shellTabs.length > 0
    ? [...state.shellTabs]
    : [fallbackShellSession]
  const activeShellTab = state?.activeShellTab !== undefined && shellTabs.includes(state.activeShellTab)
    ? state.activeShellTab
    : shellTabs[0]
  const maxShellSession = shellTabs.reduce((max, value) => Math.max(max, value), fallbackShellSession)

  return {
    launchTarget: state?.launchTarget ?? launchTarget,
    agentSession: state?.agentSession ?? 0,
    shellTabs,
    activeShellTab,
    nextShellSession: Math.max(state?.nextShellSession ?? 0, maxShellSession + 1),
    activeTopTab: state?.activeTopTab ?? 'terminal',
  }
}

function markerRank(marker: ProjectMarker): number {
  if (marker === 'favorite') return 0
  if (marker === null) return 1
  return 2
}

export const useAppStore = create<AppState>((set, get) => ({
  projects: [],
  selectedProjectPath: null,
  selectedProject: null,
  launchedProjects: new Set(),
  terminalStates: {},
  claudeStatus: {},
  shellStatus: {},
  reloadMessage: null,

  setProjects(projects) {
    set((state) => {
      const projectPaths = new Set(projects.map((project) => project.path))
      const launchedProjects = new Set(
        Array.from(state.launchedProjects).filter((projectPath) => projectPaths.has(projectPath)),
      )
      const terminalStates = Object.fromEntries(
        Object.entries(state.terminalStates).filter(([projectPath]) => projectPaths.has(projectPath)),
      )
      const claudeStatus = Object.fromEntries(
        Object.entries(state.claudeStatus).filter(([projectPath]) => projectPaths.has(projectPath)),
      )
      const shellStatus = Object.fromEntries(
        Object.entries(state.shellStatus).filter(([projectPath]) => projectPaths.has(projectPath)),
      )
      const selectedProject = resolveSelectedProject(projects, state.selectedProjectPath)
      return { projects, selectedProject, launchedProjects, terminalStates, claudeStatus, shellStatus }
    })
  },

  selectProject(project) {
    set({ selectedProjectPath: project.path, selectedProject: project })
  },

  launchProject(project, target) {
    const now = Date.now()
    set((state) => {
      const projects = state.projects.map((p) =>
        p.path === project.path ? { ...p, lastLaunched: now } : p,
      )
      const launchedProjects = new Set(state.launchedProjects)
      launchedProjects.add(project.path)
      const claudeStatus = target === 'shell'
        ? state.claudeStatus
        : { ...state.claudeStatus, [project.path]: state.claudeStatus[project.path] ?? 'waiting' }
      const shellStatus = { ...state.shellStatus, [project.path]: state.shellStatus[project.path] ?? 'waiting' }
      const terminalStates = {
        ...state.terminalStates,
        [project.path]: normalizeTerminalState(state.terminalStates[project.path], target),
      }
      terminalStates[project.path].launchTarget = target
      return {
        projects,
        selectedProjectPath: project.path,
        selectedProject: projects.find((p) => p.path === project.path) ?? { ...project, lastLaunched: now },
        launchedProjects,
        terminalStates,
        claudeStatus,
        shellStatus,
      }
    })
  },

  unlaunchProject(path) {
    set((state) => {
      const launchedProjects = new Set(state.launchedProjects)
      launchedProjects.delete(path)
      const { [path]: _terminalState, ...terminalStates } = state.terminalStates
      const { [path]: _claudeStatus, ...claudeStatus } = state.claudeStatus
      const { [path]: _shellStatus, ...shellStatus } = state.shellStatus
      return { launchedProjects, terminalStates, claudeStatus, shellStatus }
    })
  },

  setClaudeStatus(projectPath, status) {
    set((state) => ({
      claudeStatus: { ...state.claudeStatus, [projectPath]: status },
    }))
  },

  setShellStatus(projectPath, status) {
    set((state) => ({
      shellStatus: { ...state.shellStatus, [projectPath]: status },
    }))
  },

  setProjectTagOverride(projectPath, override) {
    set((state) => {
      const projects = state.projects.map((project) => {
        if (project.path !== projectPath) return project
        const tags = (override?.tags ?? project.detectedTags)
          .slice()
          .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
        const primaryTag = override?.primaryTag ?? tags[0]?.name ?? null
        return { ...project, tagOverride: override, tags, primaryTag }
      })
      return {
        projects,
        selectedProject: resolveSelectedProject(projects, state.selectedProjectPath),
      }
    })
  },

  setProjectMarker(projectPath, marker) {
    set((state) => {
      const projects = state.projects.map((project) =>
        project.path === projectPath ? { ...project, marker } : project,
      )
      return {
        projects,
        selectedProject: resolveSelectedProject(projects, state.selectedProjectPath),
      }
    })
  },

  setProjectDetectedTags(projectPath, detectedTags) {
    set((state) => {
      const projects = state.projects.map((project) => {
        if (project.path !== projectPath) return project
        const tags = (project.tagOverride?.tags ?? detectedTags)
          .slice()
          .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
        const primaryTag = project.tagOverride?.primaryTag ?? tags[0]?.name ?? null
        return { ...project, detectedTags, tags, primaryTag }
      })
      return {
        projects,
        selectedProject: resolveSelectedProject(projects, state.selectedProjectPath),
      }
    })
  },

  setActiveTopTab(projectPath, tab) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      return {
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            activeTopTab: tab,
          },
        },
      }
    })
  },

  setActiveShellTab(projectPath, shellSession) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      if (!normalized.shellTabs.includes(shellSession)) return {}
      return {
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            activeShellTab: shellSession,
          },
        },
      }
    })
  },

  createShellTab(projectPath) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      const nextShellSession = normalized.nextShellSession
      return {
        shellStatus: { ...state.shellStatus, [projectPath]: 'waiting' },
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            shellTabs: [...normalized.shellTabs, nextShellSession],
            activeShellTab: nextShellSession,
            nextShellSession: nextShellSession + 1,
          },
        },
      }
    })
  },

  closeShellTab(projectPath, shellSession) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      if (!normalized.shellTabs.includes(shellSession) || normalized.shellTabs.length <= 1) return {}
      const shellTabs = normalized.shellTabs.filter((value) => value !== shellSession)
      const activeShellTab = normalized.activeShellTab === shellSession
        ? shellTabs[Math.max(0, normalized.shellTabs.indexOf(shellSession) - 1)] ?? shellTabs[0]
        : normalized.activeShellTab
      return {
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            shellTabs,
            activeShellTab,
          },
        },
      }
    })
  },

  relaunchAgentTerminal(projectPath) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      return {
        claudeStatus: { ...state.claudeStatus, [projectPath]: 'waiting' },
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            agentSession: normalized.agentSession + 1,
          },
        },
      }
    })
  },

  relaunchShellTab(projectPath, shellSession) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      const normalized = normalizeTerminalState(current, current.launchTarget)
      const shellIndex = normalized.shellTabs.indexOf(shellSession)
      if (shellIndex === -1) return {}
      const nextShellSession = normalized.nextShellSession
      const shellTabs = [...normalized.shellTabs]
      shellTabs[shellIndex] = nextShellSession
      return {
        shellStatus: { ...state.shellStatus, [projectPath]: 'waiting' },
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...normalized,
            shellTabs,
            activeShellTab: normalized.activeShellTab === shellSession ? nextShellSession : normalized.activeShellTab,
            nextShellSession: nextShellSession + 1,
          },
        },
      }
    })
  },

  getTerminalState(projectPath) {
    return get().terminalStates[projectPath] ?? null
  },

  hydrateReloadSnapshot(snapshot) {
    set((state) => {
      const launchedProjects = new Set(snapshot.terminals.map((terminal) => terminal.projectPath))
      const terminalStates = Object.fromEntries(
        snapshot.terminals.map((terminal) => [
          terminal.projectPath,
          normalizeTerminalState(terminal, terminal.launchTarget),
        ]),
      )
      return {
        selectedProjectPath: snapshot.selectedProjectPath,
        selectedProject: resolveSelectedProject(state.projects, snapshot.selectedProjectPath),
        launchedProjects,
        terminalStates,
        reloadMessage: 'Core reloaded. Terminals reconnected.',
      }
    })
  },

  createReloadSnapshot() {
    const state = get()
    return {
      selectedProjectPath: state.selectedProjectPath,
      terminals: Array.from(state.launchedProjects)
        .map((projectPath) => {
          const terminalState = state.terminalStates[projectPath]
          if (!terminalState) return null
          return {
            projectPath,
            launchTarget: terminalState.launchTarget,
            agentSession: terminalState.agentSession,
            shellSession: terminalState.activeShellTab,
            shellTabs: terminalState.shellTabs,
            activeShellTab: terminalState.activeShellTab,
            nextShellSession: terminalState.nextShellSession,
            activeTopTab: terminalState.activeTopTab,
          }
        })
        .filter((value): value is ReloadSnapshot['terminals'][number] => value !== null),
      timestamp: Date.now(),
    }
  },

  setReloadMessage(message) {
    set({ reloadMessage: message })
  },

  sortedProjects() {
    return [...get().projects].sort((a, b) => {
      const markerDiff = markerRank(a.marker) - markerRank(b.marker)
      if (markerDiff !== 0) return markerDiff

      const launchDiff = (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0)
      if (launchDiff !== 0) return launchDiff

      return a.name.localeCompare(b.name)
    })
  },
}))

export type { LaunchTarget, ReloadSnapshot }
