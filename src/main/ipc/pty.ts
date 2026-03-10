import { ipcMain, BrowserWindow } from 'electron'
import { ptyHostClient } from '../pty/client'

export function registerPtyHandlers(getWin: () => BrowserWindow | null): void {
  ptyHostClient.onEvent((message) => {
    const win = getWin()
    if (!win || win.isDestroyed()) return
    if (message.event === 'pty:data') {
      win.webContents.send('pty:data', message.id, message.data)
      return
    }
    win.webContents.send('pty:exit', message.id, message.exitCode)
  })

  ipcMain.handle(
    'pty:create',
    async (_event, id: string, cwd: string, command: string, args: string[]) => {
      return ptyHostClient.create(id, cwd, command, args)
    }
  )

  ipcMain.on('pty:write', async (_event, id: string, data: string) => {
    await ptyHostClient.write(id, data)
  })

  ipcMain.handle('pty:resize', async (_event, id: string, cols: number, rows: number) => {
    await ptyHostClient.resize(id, cols, rows)
  })

  ipcMain.handle('pty:detach', async (_event, id: string) => {
    await ptyHostClient.detach(id)
  })

  ipcMain.handle('pty:kill', async (_event, id: string) => {
    await ptyHostClient.kill(id)
  })
}
