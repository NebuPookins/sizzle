import { ipcMain, BrowserWindow } from 'electron'
import type { ReloadSnapshot } from '../../shared/reload'
import { consumeReloadSnapshot, reloadCore } from '../appReload'

export function registerAppReloadHandlers(getMainWindow: () => BrowserWindow | null): void {
  ipcMain.handle('appReload:consumeSnapshot', async () => {
    return consumeReloadSnapshot()
  })

  ipcMain.handle('appReload:reloadCore', async (_event, snapshot: ReloadSnapshot) => {
    const win = getMainWindow()
    if (!win) throw new Error('Main window is not available')
    await reloadCore(snapshot, win)
  })
}
