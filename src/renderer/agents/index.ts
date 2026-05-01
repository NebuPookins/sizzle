import { claudeAgent } from './claude'
import { codexAgent } from './codex'
import type { LaunchTarget } from '../store/appStore'

export interface CodingAgent {
  label: string
  command: string
  getArgs(projectPath: string): Promise<string[]>
}

export function getAgent(target: LaunchTarget): CodingAgent {
  if (target === 'codex') return codexAgent
  if (target === 'claude') return claudeAgent
  return { label: target, command: target, getArgs: async () => [] }
}
