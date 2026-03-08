import { create } from 'zustand'

export interface Project {
  name: string
  path: string
  readmeFiles: string[]
  lastLaunched: number | null
}

export type LaunchTarget = 'claude' | 'codex'
type ClaudeStatus = 'working' | 'waiting'

interface AppState {
  projects: Project[]
  selectedProject: Project | null
  launchedProjects: Set<string>
  launchTargets: Record<string, LaunchTarget>
  claudeStatus: Record<string, ClaudeStatus>

  setProjects(projects: Project[]): void
  selectProject(project: Project): void
  launchProject(project: Project, target: LaunchTarget): void
  setClaudeStatus(projectPath: string, status: ClaudeStatus): void
  sortedProjects(): Project[]
}

export const useAppStore = create<AppState>((set, get) => ({
  projects: [],
  selectedProject: null,
  launchedProjects: new Set(),
  launchTargets: {},
  claudeStatus: {},

  setProjects(projects) {
    set({ projects })
  },

  selectProject(project) {
    set({ selectedProject: project })
  },

  launchProject(project, target) {
    const { launchedProjects, projects, launchTargets } = get()
    const now = Date.now()
    const updated = projects.map((p) =>
      p.path === project.path ? { ...p, lastLaunched: now } : p
    )
    const newLaunched = new Set(launchedProjects)
    newLaunched.add(project.path)
    set({
      projects: updated,
      selectedProject: { ...project, lastLaunched: now },
      launchedProjects: newLaunched,
      launchTargets: { ...launchTargets, [project.path]: target },
    })
  },

  setClaudeStatus(projectPath, status) {
    set((state) => ({
      claudeStatus: { ...state.claudeStatus, [projectPath]: status },
    }))
  },

  sortedProjects() {
    return [...get().projects].sort((a, b) => {
      if (!a.lastLaunched && !b.lastLaunched) return a.name.localeCompare(b.name)
      return (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0)
    })
  },
}))
