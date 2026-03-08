import type { CodingAgent } from './index'

export const claudeAgent: CodingAgent = {
  label: 'Claude Code',
  command: 'claude',
  async getArgs(projectPath: string): Promise<string[]> {
    const hasSession = await window.sizzle.claudeHasSession(projectPath)
    return hasSession ? ['--continue'] : []
  },
}
