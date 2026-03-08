import { ipcMain } from 'electron'
import fs from 'fs'
import path from 'path'
import { scanForProjects } from '../scanner'

const ROOT_DIR = '/mnt/safe/home/nebu/myPrograms'

export function registerScannerHandlers(): void {
  ipcMain.handle('scanner:scan', async () => {
    return await scanForProjects(ROOT_DIR)
  })

  ipcMain.handle('markdown:getFiles', async (_event, projectPath: string) => {
    try {
      const entries = fs.readdirSync(projectPath)
      return entries
        .filter((e) => {
          const lower = e.toLowerCase()
          return lower.endsWith('.md') || lower.endsWith('.txt') || lower.endsWith('.rst')
        })
        .sort((a, b) => {
          // README first
          const aIsReadme = a.toLowerCase().startsWith('readme')
          const bIsReadme = b.toLowerCase().startsWith('readme')
          if (aIsReadme && !bIsReadme) return -1
          if (!aIsReadme && bIsReadme) return 1
          return a.localeCompare(b)
        })
        .map((e) => path.join(projectPath, e))
    } catch {
      return []
    }
  })

  ipcMain.handle('markdown:readFile', async (_event, filePath: string) => {
    try {
      return fs.readFileSync(filePath, 'utf-8')
    } catch {
      return null
    }
  })
}
