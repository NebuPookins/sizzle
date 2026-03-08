import { contextBridge, ipcRenderer } from 'electron'

export interface ScannedProject {
  name: string
  path: string
  readmeFiles: string[]
}

export interface ProjectMeta {
  lastLaunched: number | null
}

const api = {
  // Scanner
  scanProjects: (): Promise<ScannedProject[]> =>
    ipcRenderer.invoke('scanner:scan'),

  getMarkdownFiles: (projectPath: string): Promise<string[]> =>
    ipcRenderer.invoke('markdown:getFiles', projectPath),

  readMarkdownFile: (filePath: string): Promise<string | null> =>
    ipcRenderer.invoke('markdown:readFile', filePath),

  // Metadata
  getMetadata: (projectPath: string): Promise<ProjectMeta> =>
    ipcRenderer.invoke('metadata:get', projectPath),

  getAllMetadata: (): Promise<Record<string, ProjectMeta>> =>
    ipcRenderer.invoke('metadata:getAll'),

  setLastLaunched: (projectPath: string): Promise<void> =>
    ipcRenderer.invoke('metadata:setLastLaunched', projectPath),

  // PTY
  ptyCreate: (id: string, cwd: string, command: string, args: string[]): Promise<void> =>
    ipcRenderer.invoke('pty:create', id, cwd, command, args),

  ptyWrite: (id: string, data: string): Promise<void> =>
    ipcRenderer.invoke('pty:write', id, data),

  ptyResize: (id: string, cols: number, rows: number): Promise<void> =>
    ipcRenderer.invoke('pty:resize', id, cols, rows),

  ptyKill: (id: string): Promise<void> =>
    ipcRenderer.invoke('pty:kill', id),

  onPtyData: (callback: (id: string, data: string) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, id: string, data: string) =>
      callback(id, data)
    ipcRenderer.on('pty:data', handler)
    return () => ipcRenderer.removeListener('pty:data', handler)
  },

  onPtyExit: (callback: (id: string, exitCode: number) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, id: string, exitCode: number) =>
      callback(id, exitCode)
    ipcRenderer.on('pty:exit', handler)
    return () => ipcRenderer.removeListener('pty:exit', handler)
  },
}

contextBridge.exposeInMainWorld('sizzle', api)
