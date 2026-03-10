import { ipcMain } from 'electron'
import {
  getMetadata,
  setLastLaunched,
  getAllMetadata,
  setTagOverride,
  setProjectMarker,
  ProjectMarker,
  ProjectTagOverride,
} from '../store/metadata'

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

  ipcMain.handle('metadata:setProjectMarker', async (_event, projectPath: string, marker: ProjectMarker) => {
    return setProjectMarker(projectPath, marker)
  })
}
