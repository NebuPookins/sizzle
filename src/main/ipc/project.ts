import { ipcMain } from 'electron'
import fs from 'fs'
import path from 'path'
import os from 'os'
import { getScanSettings, setScanSettings, renameProjectMetadata } from '../store/metadata'

export interface MoveRenameResult {
  success: boolean
  error?: string
  changes: string[]
}

function pathToClaudeDir(projectPath: string): string {
  return projectPath.replace(/\//g, '-')
}

function isUnderScanRoot(targetPath: string, scanRoots: string[]): boolean {
  const normalized = path.resolve(targetPath)
  return scanRoots.some((root) => {
    const normalizedRoot = path.resolve(root)
    return normalized === normalizedRoot || normalized.startsWith(`${normalizedRoot}${path.sep}`)
  })
}

export function registerProjectHandlers(): void {
  ipcMain.handle(
    'project:moveRename',
    async (_event, oldPath: string, newPath: string): Promise<MoveRenameResult> => {
      const changes: string[] = []

      try {
        fs.renameSync(oldPath, newPath)
        changes.push(`Moved directory:\n  ${oldPath}\n  → ${newPath}`)

        renameProjectMetadata(oldPath, newPath)

        const settings = getScanSettings()
        const wasManual = settings.manualProjectRoots.includes(oldPath)
        const newManualRoots = settings.manualProjectRoots.map((r) => (r === oldPath ? newPath : r))
        const newIgnoreRoots = settings.ignoreRoots.map((r) => (r === oldPath ? newPath : r))
        const newScanRoots = settings.scanRoots.map((r) => (r === oldPath ? newPath : r))

        const willBeVisible = isUnderScanRoot(newPath, newScanRoots) || newManualRoots.includes(newPath)
        if (!willBeVisible) {
          newManualRoots.push(newPath)
        }

        setScanSettings({
          scanRoots: newScanRoots,
          ignoreRoots: newIgnoreRoots,
          manualProjectRoots: newManualRoots,
        })

        const claudeProjectsDir = path.join(os.homedir(), '.claude', 'projects')
        const oldClaudeDir = path.join(claudeProjectsDir, pathToClaudeDir(oldPath))
        const newClaudeDir = path.join(claudeProjectsDir, pathToClaudeDir(newPath))

        if (fs.existsSync(oldClaudeDir)) {
          fs.renameSync(oldClaudeDir, newClaudeDir)
          changes.push(`Moved Claude project data:\n  ${oldClaudeDir}\n  → ${newClaudeDir}`)
        }

        const codexConfigPath = path.join(os.homedir(), '.codex', 'config.toml')
        if (fs.existsSync(codexConfigPath)) {
          let content = fs.readFileSync(codexConfigPath, 'utf-8')
          const oldKey = `[projects."${oldPath}"]`
          const newKey = `[projects."${newPath}"]`
          if (content.includes(oldKey)) {
            content = content.split(oldKey).join(newKey)
            fs.writeFileSync(codexConfigPath, content, 'utf-8')
            changes.push(`Updated Codex config (${codexConfigPath}):\n  replaced "${oldPath}" with "${newPath}"`)
          }
        }

        return { success: true, changes }
      } catch (error) {
        return {
          success: false,
          error: error instanceof Error ? error.message : String(error),
          changes,
        }
      }
    },
  )
}
