import { app, BrowserWindow, shell } from 'electron'
import path from 'path'
import { registerScannerHandlers } from './ipc/scanner'
import { registerPtyHandlers } from './ipc/pty'
import { registerMetadataHandlers } from './ipc/metadata'
import { registerClaudeHandlers } from './ipc/claude'
import { registerAppReloadHandlers } from './ipc/appReload'
import { getQuitMode, signalReloadReady } from './appReload'
import { ptyHostClient } from './pty/client'

let mainWindow: BrowserWindow | null = null

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    backgroundColor: '#1a1a2e',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  })

  // Open external links in browser
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url)
    return { action: 'deny' }
  })

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL)
    mainWindow.webContents.openDevTools()
  } else {
    mainWindow.loadFile(path.join(__dirname, '../renderer/index.html'))
  }

  mainWindow.webContents.once('did-finish-load', () => {
    signalReloadReady()
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

app.whenReady().then(() => {
  registerScannerHandlers()
  registerPtyHandlers(() => mainWindow)
  registerMetadataHandlers()
  registerClaudeHandlers()
  registerAppReloadHandlers(() => mainWindow)
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => {
  if (getQuitMode() === 'reload') {
    ptyHostClient.disconnect()
    return
  }
  void ptyHostClient.shutdown()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
