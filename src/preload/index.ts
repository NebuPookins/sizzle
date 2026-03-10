import { contextBridge, ipcRenderer } from 'electron'

export interface ProjectTag {
  name: string
  score: number
}

export interface ScannedProject {
  name: string
  path: string
  readmeFiles: string[]
  detectedTags: ProjectTag[]
}

export interface ProjectTagOverride {
  tags: ProjectTag[]
  primaryTag: string | null
}

export type ProjectMarker = 'favorite' | 'ignored' | null

export interface ProjectMeta {
  lastLaunched: number | null
  tagOverride: ProjectTagOverride | null
  marker: ProjectMarker
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

export interface PtyOpenResult {
  replay: string
  exitCode: number | null
}

const api = {
  defaultShell: process.env.SHELL || process.env.COMSPEC || '/bin/bash',

  // Scanner
  scanProjects: (): Promise<ScannedProject[]> =>
    ipcRenderer.invoke('scanner:scan'),

  rescanProjectTags: (projectPath: string): Promise<ProjectTag[]> =>
    ipcRenderer.invoke('scanner:rescanProject', projectPath),

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

  setTagOverride: (projectPath: string, override: ProjectTagOverride | null): Promise<ProjectMeta> =>
    ipcRenderer.invoke('metadata:setTagOverride', projectPath, override),

  setProjectMarker: (projectPath: string, marker: ProjectMarker): Promise<ProjectMeta> =>
    ipcRenderer.invoke('metadata:setProjectMarker', projectPath, marker),

  // Claude
  claudeHasSession: (projectPath: string): Promise<boolean> =>
    ipcRenderer.invoke('claude:hasSession', projectPath),

  // PTY
  ptyCreate: (id: string, cwd: string, command: string, args: string[]): Promise<PtyOpenResult> =>
    ipcRenderer.invoke('pty:create', id, cwd, command, args),

  ptyWrite: (id: string, data: string): void =>
    ipcRenderer.send('pty:write', id, data),

  ptyResize: (id: string, cols: number, rows: number): Promise<void> =>
    ipcRenderer.invoke('pty:resize', id, cols, rows),

  ptyDetach: (id: string): Promise<void> =>
    ipcRenderer.invoke('pty:detach', id),

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
