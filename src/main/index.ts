import { app, BrowserWindow, ipcMain, shell, WebContents } from 'electron'
import path from 'path'
import { registerScannerHandlers } from './ipc/scanner'
import { registerPtyHandlers } from './ipc/pty'
import { registerMetadataHandlers } from './ipc/metadata'
import { registerClaudeHandlers } from './ipc/claude'
import { registerAppReloadHandlers } from './ipc/appReload'
import { getQuitMode, signalReloadReady } from './appReload'
import { ptyHostClient } from './pty/client'

let mainWindow: BrowserWindow | null = null

function configureExternalLinks(contents: WebContents): void {
  contents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url)
    return { action: 'deny' }
  })
}

async function loadMainWindow(window: BrowserWindow): Promise<void> {
  if (process.env.ELECTRON_RENDERER_URL) {
    await window.loadURL(process.env.ELECTRON_RENDERER_URL)
    return
  }

  await window.loadFile(path.join(__dirname, '../renderer/index.html'))
}

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
      webviewTag: true,
    },
  })

  configureExternalLinks(mainWindow.webContents)
  mainWindow.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL) => {
    console.error('Renderer failed to load', { errorCode, errorDescription, validatedURL })
  })

  void loadMainWindow(mainWindow)
    .then(() => {
      signalReloadReady()
    })
    .catch((error) => {
      console.error('Failed to load main window', error)
    })

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

app.whenReady().then(() => {
  app.on('web-contents-created', (_event, contents) => {
    configureExternalLinks(contents)
  })

  registerScannerHandlers()
  registerPtyHandlers(() => mainWindow)
  registerMetadataHandlers()
  registerClaudeHandlers()
  registerAppReloadHandlers(() => mainWindow)

  ipcMain.on('window:setTitle', (_event, projectName: string | null) => {
    if (!mainWindow) return
    mainWindow.setTitle(projectName ? `Sizzle - ${projectName}` : 'Sizzle')
  })

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
