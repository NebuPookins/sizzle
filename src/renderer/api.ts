import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

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

export interface ArchiveTreeNode {
  name: string
  path: string
  isDirectory: boolean
  children?: ArchiveTreeNode[]
}

export type FilePreviewKind = 'text' | 'media' | 'archive' | 'unsupported' | 'tooLarge' | 'error'

export interface FilePreview {
  kind: FilePreviewKind
  content?: string
  mimeType?: string
  size?: number
  message?: string
  archiveTree?: ArchiveTreeNode[]
}

export interface ProjectRepositoryInfo {
  isGitRepo: boolean
  githubUrl: string | null
}

export interface GitFileChange {
  status: string
  path: string
  origPath?: string
}

export interface GitStatus {
  branch: string | null
  upstream: string | null
  ahead: number
  behind: number
  staged: GitFileChange[]
  unstaged: GitFileChange[]
  untracked: string[]
  isDetached: boolean
}

export interface MoveRenameResult {
  success: boolean
  error?: string
  changes: string[]
}

export interface PtyCreateResult {
  replay: string
  exitCode: number | null
}

// Scanner
export const scanProjects = (): Promise<ScannedProject[]> =>
  invoke('scan_projects')

export const rescanProjectTags = (projectPath: string): Promise<ProjectTag[]> =>
  invoke('rescan_project_tags', { projectPath })

export const getScanSettings = (): Promise<ScanSettings> =>
  invoke('get_scan_settings')

export const setScanSettings = (settings: ScanSettings): Promise<ScanSettings> =>
  invoke('set_scan_settings', { settings })

export const addIgnoreRoot = (rootPath: string): Promise<ScanSettings> =>
  invoke('add_ignore_root', { rootPath })

// Files / Markdown
export const getMarkdownFiles = (projectPath: string): Promise<string[]> =>
  invoke('get_markdown_files', { projectPath })

export const readMarkdownFile = (filePath: string): Promise<string | null> =>
  invoke('read_markdown_file', { filePath })

export const listDirectory = (projectPath: string, directoryPath?: string): Promise<FileSystemEntry[]> =>
  invoke('list_directory', { projectPath, directoryPath })

export const previewFile = (projectPath: string, filePath: string): Promise<FilePreview> =>
  invoke('preview_file', { projectPath, filePath })

// Git
export const getProjectRepositoryInfo = (projectPath: string): Promise<ProjectRepositoryInfo> =>
  invoke('get_project_repository_info', { projectPath })

export const getGitStatus = (projectPath: string): Promise<GitStatus | null> =>
  invoke('get_git_status', { projectPath })

// Metadata
export const getMetadata = (projectPath: string): Promise<ProjectMeta> =>
  invoke('get_metadata', { projectPath })

export const getAllMetadata = (): Promise<Record<string, ProjectMeta>> =>
  invoke('get_all_metadata')

export const setLastLaunched = (projectPath: string): Promise<void> =>
  invoke('set_last_launched', { projectPath })

export const setTagOverride = (projectPath: string, override: ProjectTagOverride | null): Promise<ProjectMeta> =>
  invoke('set_tag_override', { projectPath, override })

export const setProjectMarker = (projectPath: string, marker: ProjectMarker): Promise<ProjectMeta> =>
  invoke('set_project_marker', { projectPath, marker })

// Claude
export const claudeHasSession = (projectPath: string): Promise<boolean> =>
  invoke('claude_has_session', { projectPath })

// PTY
export const ptyCreate = (id: string, cwd: string, command: string, args: string[]): Promise<PtyCreateResult> =>
  invoke('pty_create', { id, cwd, command, args })

export const ptyWrite = (id: string, data: string): Promise<void> =>
  invoke('pty_write', { id, data })

export const ptyResize = (id: string, cols: number, rows: number): Promise<void> =>
  invoke('pty_resize', { id, cols, rows })

export const ptyDetach = (id: string): Promise<void> =>
  invoke('pty_detach', { id })

export const ptyKill = (id: string): Promise<void> =>
  invoke('pty_kill', { id })

export const ptyListSessions = (): Promise<string[]> =>
  invoke('pty_list_sessions')

// Events
export function onPtyData(callback: (id: string, data: string) => void): Promise<UnlistenFn> {
  return listen<PtyDataPayload>('pty:data', (event) => {
    callback(event.payload.id, event.payload.data)
  })
}

export function onPtyExit(callback: (id: string, exitCode: number) => void): Promise<UnlistenFn> {
  return listen<PtyExitPayload>('pty:exit', (event) => {
    callback(event.payload.id, event.payload.exitCode)
  })
}

interface PtyDataPayload {
  id: string
  data: string
}

interface PtyExitPayload {
  id: string
  exitCode: number
}

// Shell / defaults
export const getDefaultShell = (): Promise<string> =>
  invoke<string>('get_default_shell')

// Projects
export const moveRenameProject = (oldPath: string, newPath: string): Promise<MoveRenameResult> =>
  invoke('move_rename_project', { oldPath, newPath })

// App reload (simplified for Tauri — just restart or no-op)
import type { ReloadSnapshot } from '../shared/reload'

export const consumeReloadSnapshot = async (): Promise<ReloadSnapshot | null> => {
  // No reload mechanism needed in Tauri — full restart replaces the process.
  // This exists for compatibility with the existing reload flow.
  return null
}

export const reloadCore = async (_snapshot: ReloadSnapshot): Promise<void> => {
  // In Tauri, a full app restart would go here.
  // For now, this is a no-op — the user can relaunch the app.
  throw new Error('Core reload not supported in Tauri build. Relaunch the app instead.')
}

// Window title
import { getCurrentWindow } from '@tauri-apps/api/window'

export const setWindowTitle = (projectName: string | null): Promise<void> =>
  getCurrentWindow().setTitle(projectName ? `Sizzle - ${projectName}` : 'Sizzle')
