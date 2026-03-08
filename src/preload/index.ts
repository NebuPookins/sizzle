import { contextBridge, ipcRenderer } from 'electron'

export interface ScannedProject {
  name: string
  path: string
  readmeFiles: string[]
}

export interface ProjectMeta {
  lastLaunched: number | null
}

export interface ScanSettings {
  scanRoots: string[]
  ignoreRoots: string[]
  manualProjectRoots: string[]
}

export interface FileSystemEntry {
  name: string
  path: string
  isDirectory: boolean
}

export type FilePreviewKind = 'text' | 'media' | 'unsupported' | 'tooLarge' | 'error'

export interface FilePreview {
  kind: FilePreviewKind
  content?: string
  mimeType?: string
  size?: number
  message?: string
}

const api = {
  defaultShell: process.env.SHELL || process.env.COMSPEC || '/bin/bash',

  // Scanner
  scanProjects: (): Promise<ScannedProject[]> =>
    ipcRenderer.invoke('scanner:scan'),

  getScanSettings: (): Promise<ScanSettings> =>
    ipcRenderer.invoke('scanner:getSettings'),

  setScanSettings: (settings: ScanSettings): Promise<ScanSettings> =>
    ipcRenderer.invoke('scanner:setSettings', settings),

  addIgnoreRoot: (rootPath: string): Promise<ScanSettings> =>
    ipcRenderer.invoke('scanner:addIgnoreRoot', rootPath),

  pickDirectory: (): Promise<string | null> =>
    ipcRenderer.invoke('scanner:pickDirectory'),

  getMarkdownFiles: (projectPath: string): Promise<string[]> =>
    ipcRenderer.invoke('markdown:getFiles', projectPath),

  readMarkdownFile: (filePath: string): Promise<string | null> =>
    ipcRenderer.invoke('markdown:readFile', filePath),

  listDirectory: (projectPath: string, directoryPath?: string): Promise<FileSystemEntry[]> =>
    ipcRenderer.invoke('files:listDirectory', projectPath, directoryPath),

  previewFile: (projectPath: string, filePath: string): Promise<FilePreview> =>
    ipcRenderer.invoke('files:preview', projectPath, filePath),

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
