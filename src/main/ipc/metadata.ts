import { ipcMain } from 'electron'
import { getMetadata, setLastLaunched, getAllMetadata } from '../store/metadata'

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
}
