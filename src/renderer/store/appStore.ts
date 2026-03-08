import { create } from 'zustand'

export interface Project {
  name: string
  path: string
  readmeFiles: string[]
  lastLaunched: number | null
}

type ClaudeStatus = 'working' | 'waiting'

interface AppState {
  projects: Project[]
  selectedProject: Project | null
  launchedProjects: Set<string>
  claudeStatus: Record<string, ClaudeStatus>

  setProjects(projects: Project[]): void
  selectProject(project: Project): void
  launchProject(project: Project): void
  setClaudeStatus(projectPath: string, status: ClaudeStatus): void
  sortedProjects(): Project[]
}

export const useAppStore = create<AppState>((set, get) => ({
  projects: [],
  selectedProject: null,
  launchedProjects: new Set(),
  claudeStatus: {},

  setProjects(projects) {
    set({ projects })
  },

  selectProject(project) {
    set({ selectedProject: project })
  },

  launchProject(project) {
    const { launchedProjects, projects } = get()
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
