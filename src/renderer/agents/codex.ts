import type { CodingAgent } from './index'

export const codexAgent: CodingAgent = {
  label: 'Codex',
  command: 'codex',
  async getArgs(_projectPath: string): Promise<string[]> {
    return []
  },
}
