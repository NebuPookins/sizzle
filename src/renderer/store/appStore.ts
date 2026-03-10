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
  shellSession: number
  activeTopTab: 'terminal' | 'explorer' | string
}

type ClaudeStatus = 'working' | 'waiting'

interface AppState {
  projects: Project[]
  selectedProjectPath: string | null
  selectedProject: Project | null
  launchedProjects: Set<string>
  terminalStates: Record<string, ProjectTerminalState>
  claudeStatus: Record<string, ClaudeStatus>
  reloadMessage: string | null

  setProjects(projects: Project[]): void
  selectProject(project: Project): void
  launchProject(project: Project, target: LaunchTarget): void
  unlaunchProject(path: string): void
  setClaudeStatus(projectPath: string, status: ClaudeStatus): void
  setProjectTagOverride(projectPath: string, override: ProjectTagOverride | null): void
  setProjectMarker(projectPath: string, marker: ProjectMarker): void
  setProjectDetectedTags(projectPath: string, detectedTags: ProjectTag[]): void
  setActiveTopTab(projectPath: string, tab: 'terminal' | 'explorer' | string): void
  relaunchTerminal(projectPath: string, which: 'agent' | 'shell'): void
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
    shellSession: 0,
    activeTopTab: 'terminal',
  }
}

export const useAppStore = create<AppState>((set, get) => ({
  projects: [],
  selectedProjectPath: null,
  selectedProject: null,
  launchedProjects: new Set(),
  terminalStates: {},
  claudeStatus: {},
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
      const selectedProject = resolveSelectedProject(projects, state.selectedProjectPath)
      return { projects, selectedProject, launchedProjects, terminalStates, claudeStatus }
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
      const terminalStates = {
        ...state.terminalStates,
        [project.path]: {
          ...(state.terminalStates[project.path] ?? defaultTerminalState(target)),
          launchTarget: target,
        },
      }
      return {
        projects,
        selectedProjectPath: project.path,
        selectedProject: projects.find((p) => p.path === project.path) ?? { ...project, lastLaunched: now },
        launchedProjects,
        terminalStates,
      }
    })
  },

  unlaunchProject(path) {
    set((state) => {
      const launchedProjects = new Set(state.launchedProjects)
      launchedProjects.delete(path)
      const { [path]: _terminalState, ...terminalStates } = state.terminalStates
      const { [path]: _claudeStatus, ...claudeStatus } = state.claudeStatus
      return { launchedProjects, terminalStates, claudeStatus }
    })
  },

  setClaudeStatus(projectPath, status) {
    set((state) => ({
      claudeStatus: { ...state.claudeStatus, [projectPath]: status },
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
      return {
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...current,
            activeTopTab: tab,
          },
        },
      }
    })
  },

  relaunchTerminal(projectPath, which) {
    set((state) => {
      const current = state.terminalStates[projectPath]
      if (!current) return {}
      return {
        terminalStates: {
          ...state.terminalStates,
          [projectPath]: {
            ...current,
            agentSession: which === 'agent' ? current.agentSession + 1 : current.agentSession,
            shellSession: which === 'shell' ? current.shellSession + 1 : current.shellSession,
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
          {
            launchTarget: terminal.launchTarget,
            agentSession: terminal.agentSession,
            shellSession: terminal.shellSession,
            activeTopTab: terminal.activeTopTab,
          },
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
            shellSession: terminalState.shellSession,
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
      if (!a.lastLaunched && !b.lastLaunched) return a.name.localeCompare(b.name)
      return (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0)
    })
  },
}))

export type { LaunchTarget, ReloadSnapshot }
