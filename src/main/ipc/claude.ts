import { ipcMain } from 'electron'
import * as fs from 'fs'
import * as os from 'os'
import * as path from 'path'

function hasClaudeSession(projectPath: string): boolean {
  const encoded = projectPath.replace(/\//g, '-')
  const sessionDir = path.join(os.homedir(), '.claude', 'projects', encoded)
  if (!fs.existsSync(sessionDir)) return false
  return fs.readdirSync(sessionDir).some(f => f.endsWith('.jsonl'))
}

export function registerClaudeHandlers(): void {
  ipcMain.handle('claude:hasSession', async (_event, projectPath: string) => {
    return hasClaudeSession(projectPath)
  })
}
