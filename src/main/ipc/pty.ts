import { ipcMain, BrowserWindow } from 'electron'
import { createPty, writePty, resizePty, killPty } from '../pty/manager'

export function registerPtyHandlers(getWin: () => BrowserWindow | null): void {
  ipcMain.handle(
    'pty:create',
    async (_event, id: string, cwd: string, command: string, args: string[]) => {
      const win = getWin()
      if (!win) return
      createPty(id, cwd, command, args, win)
    }
  )

  ipcMain.on('pty:write', (_event, id: string, data: string) => {
    writePty(id, data)
  })

  ipcMain.handle('pty:resize', async (_event, id: string, cols: number, rows: number) => {
    resizePty(id, cols, rows)
  })

  ipcMain.handle('pty:kill', async (_event, id: string) => {
    killPty(id)
  })
}
