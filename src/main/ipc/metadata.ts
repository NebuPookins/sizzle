import { ipcMain } from 'electron'
import { getMetadata, setLastLaunched, getAllMetadata, setTagOverride, ProjectTagOverride } from '../store/metadata'

export function registerMetadataHandlers(): void {
  ipcMain.handle('metadata:get', async (_event, projectPath: string) => {
    return getMetadata(projectPath)
  })

  ipcMain.handle('metadata:getAll', async () => {
    return getAllMetadata()
  })

  ipcMain.handle('metadata:setLastLaunched', async (_event, projectPath: string) => {
    setLastLaunched(projectPath)
  })

  ipcMain.handle('metadata:setTagOverride', async (_event, projectPath: string, override: ProjectTagOverride | null) => {
    return setTagOverride(projectPath, override)
  })
}
