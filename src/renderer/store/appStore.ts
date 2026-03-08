import { create } from 'zustand'
import type { ProjectTag, ProjectTagOverride } from '../../preload'

export interface Project {
  name: string
  path: string
  readmeFiles: string[]
  lastLaunched: number | null
  detectedTags: ProjectTag[]
  tags: ProjectTag[]
  primaryTag: string | null
  tagOverride: ProjectTagOverride | null
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
  setProjectTagOverride(projectPath: string, override: ProjectTagOverride | null): void
  setProjectDetectedTags(projectPath: string, detectedTags: ProjectTag[]): void
  sortedProjects(): Project[]
}

export const useAppStore = create<AppState>((set, get) => ({
  projects: [],
  selectedProject: null,
  launchedProjects: new Set(),
  launchTargets: {},
  claudeStatus: {},

  setProjects(projects) {
    const selectedPath = get().selectedProject?.path
    const selectedProject = selectedPath
      ? projects.find((project) => project.path === selectedPath) ?? null
      : null
    set({ projects, selectedProject })
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

  setProjectTagOverride(projectPath, override) {
    set((state) => {
      const projects = state.projects.map((project) => {
        if (project.path !== projectPath) return project
        const tags = (override?.tags ?? project.detectedTags).slice().sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
        const primaryTag = override?.primaryTag
          ?? tags[0]?.name
          ?? null
        return {
          ...project,
          tagOverride: override,
          tags,
          primaryTag,
        }
      })

      const selectedProject = state.selectedProject?.path === projectPath
        ? projects.find((project) => project.path === projectPath) ?? null
        : state.selectedProject

      return { projects, selectedProject }
    })
  },

  setProjectDetectedTags(projectPath, detectedTags) {
    set((state) => {
      const projects = state.projects.map((project) => {
        if (project.path !== projectPath) return project
        const tags = (project.tagOverride?.tags ?? detectedTags).slice().sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
        const primaryTag = project.tagOverride?.primaryTag ?? tags[0]?.name ?? null
        return { ...project, detectedTags, tags, primaryTag }
      })

      const selectedProject = state.selectedProject?.path === projectPath
        ? projects.find((p) => p.path === projectPath) ?? null
        : state.selectedProject

      return { projects, selectedProject }
    })
  },

  sortedProjects() {
    return [...get().projects].sort((a, b) => {
      if (!a.lastLaunched && !b.lastLaunched) return a.name.localeCompare(b.name)
      return (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0)
    })
  },
}))
