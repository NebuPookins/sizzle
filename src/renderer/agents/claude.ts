import type { CodingAgent } from './index'
import { claudeHasSession } from '../api'

export const claudeAgent: CodingAgent = {
  label: 'Claude Code',
  command: 'claude',
  async getArgs(projectPath: string): Promise<string[]> {
    const hasSession = await claudeHasSession(projectPath)
    return hasSession ? ['--continue'] : []
  },
}
