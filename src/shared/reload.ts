export type LaunchTarget = 'claude' | 'codex' | 'shell'

export interface ProjectTerminalStateSnapshot {
  projectPath: string
  launchTarget: LaunchTarget
  agentSession: number
  shellSession: number
  shellTabs: number[]
  activeShellTab: number
  nextShellSession: number
  activeTopTab: string
}

export interface ReloadSnapshot {
  selectedProjectPath: string | null
  terminals: ProjectTerminalStateSnapshot[]
  timestamp: number
}
