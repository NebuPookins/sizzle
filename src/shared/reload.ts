export type LaunchTarget = string

export interface ProjectTerminalStateSnapshot {
  projectPath: string
  launchTarget: LaunchTarget
  agentSession: number
  shellSession: number
  shellTabs: number[]
  activeShellTab: number
  nextShellSession: number
  activeTopTab: string
  focusedPane: 'agent' | 'shell' | null
  initialCommand?: string
  customAgent?: { label: string; command: string }
}

export interface ReloadSnapshot {
  selectedProjectPath: string | null
  terminals: ProjectTerminalStateSnapshot[]
  timestamp: number
}
