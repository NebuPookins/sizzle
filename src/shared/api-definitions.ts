/**
 * Central registry of every backend command and event the frontend can use.
 *
 * Each command bundles metadata (for manifest generation) with a typed
 * invoke function.  The real invoke() from @tauri-apps/api/core is ONLY
 * imported here — app code calls through COMMANDS.xxx.invoke() and never
 * touches invoke() directly.
 *
 * The Vite plugin reads this file to compute the frontend manifest, which
 * is runtime-compared against the backend manifest (from build.rs) to
 * detect when the two sides drift apart.
 */

import { invoke } from '@tauri-apps/api/core'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ScannedProject {
  name: string
  path: string
  readmeFiles: string[]
  detectedTags: ProjectTag[]
}

export interface ProjectTag {
  name: string
  score: number
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

export interface AgentPreset {
  label: string
  command: string
}

export interface PtyCreateResult {
  replay: string
  exitCode: number | null
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export const COMMANDS = {
  get_api_manifest: {
    name: 'get_api_manifest' as const,
    params: [] as const,
    invoke: () => invoke<{ format: number; commands: { name: string; args: string[] }[]; events: { name: string }[] }>('get_api_manifest', {}),
  },

  scan_projects: {
    name: 'scan_projects' as const,
    params: [] as const,
    invoke: () => invoke<ScannedProject[]>('scan_projects', {}),
  },

  rescan_project_tags: {
    name: 'rescan_project_tags' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<ProjectTag[]>('rescan_project_tags', { projectPath }),
  },

  get_scan_settings: {
    name: 'get_scan_settings' as const,
    params: [] as const,
    invoke: () => invoke<ScanSettings>('get_scan_settings', {}),
  },

  set_scan_settings: {
    name: 'set_scan_settings' as const,
    params: ['settings'] as const,
    invoke: (settings: ScanSettings) => invoke<ScanSettings>('set_scan_settings', { settings }),
  },

  add_ignore_root: {
    name: 'add_ignore_root' as const,
    params: ['rootPath'] as const,
    invoke: (rootPath: string) => invoke<ScanSettings>('add_ignore_root', { rootPath }),
  },

  get_markdown_files: {
    name: 'get_markdown_files' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<string[]>('get_markdown_files', { projectPath }),
  },

  get_project_detail: {
    name: 'get_project_detail' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<{ markdownFiles: string[]; isGitRepo: boolean; githubUrl: string | null }>('get_project_detail', { projectPath }),
  },

  read_markdown_file: {
    name: 'read_markdown_file' as const,
    params: ['filePath'] as const,
    invoke: (filePath: string) => invoke<string | null>('read_markdown_file', { filePath }),
  },

  write_markdown_file: {
    name: 'write_markdown_file' as const,
    params: ['filePath', 'content'] as const,
    invoke: (filePath: string, content: string) => invoke<null>('write_markdown_file', { filePath, content }),
  },

  list_directory: {
    name: 'list_directory' as const,
    params: ['projectPath', 'directoryPath'] as const,
    invoke: (projectPath: string, directoryPath?: string) => invoke<FileSystemEntry[]>('list_directory', { projectPath, directoryPath }),
  },

  preview_file: {
    name: 'preview_file' as const,
    params: ['projectPath', 'filePath'] as const,
    invoke: (projectPath: string, filePath: string) => invoke<FilePreview>('preview_file', { projectPath, filePath }),
  },

  get_project_repository_info: {
    name: 'get_project_repository_info' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<ProjectRepositoryInfo>('get_project_repository_info', { projectPath }),
  },

  get_git_status: {
    name: 'get_git_status' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<GitStatus | null>('get_git_status', { projectPath }),
  },

  get_metadata: {
    name: 'get_metadata' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<ProjectMeta>('get_metadata', { projectPath }),
  },

  get_all_metadata: {
    name: 'get_all_metadata' as const,
    params: [] as const,
    invoke: () => invoke<Record<string, ProjectMeta>>('get_all_metadata', {}),
  },

  set_last_launched: {
    name: 'set_last_launched' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<void>('set_last_launched', { projectPath }),
  },

  set_tag_override: {
    name: 'set_tag_override' as const,
    params: ['projectPath', 'overrideVal'] as const,
    invoke: (projectPath: string, override: ProjectTagOverride | null) => invoke<ProjectMeta>('set_tag_override', { projectPath, overrideVal: override }),
  },

  set_project_marker: {
    name: 'set_project_marker' as const,
    params: ['projectPath', 'marker'] as const,
    invoke: (projectPath: string, marker: ProjectMarker) => invoke<ProjectMeta>('set_project_marker', { projectPath, marker }),
  },

  move_rename_project: {
    name: 'move_rename_project' as const,
    params: ['oldPath', 'newPath'] as const,
    invoke: (oldPath: string, newPath: string) => invoke<MoveRenameResult>('move_rename_project', { oldPath, newPath }),
  },

  claude_has_session: {
    name: 'claude_has_session' as const,
    params: ['projectPath'] as const,
    invoke: (projectPath: string) => invoke<boolean>('claude_has_session', { projectPath }),
  },

  command_exists: {
    name: 'command_exists' as const,
    params: ['command'] as const,
    invoke: (command: string) => invoke<boolean>('command_exists', { command }),
  },

  get_default_shell: {
    name: 'get_default_shell' as const,
    params: [] as const,
    invoke: () => invoke<string>('get_default_shell', {}),
  },

  pty_create: {
    name: 'pty_create' as const,
    params: ['id', 'cwd', 'command', 'args'] as const,
    invoke: (id: string, cwd: string, command: string, args: string[]) => invoke<PtyCreateResult>('pty_create', { id, cwd, command, args }),
  },

  pty_write: {
    name: 'pty_write' as const,
    params: ['id', 'data'] as const,
    invoke: (id: string, data: string) => invoke<void>('pty_write', { id, data }),
  },

  pty_resize: {
    name: 'pty_resize' as const,
    params: ['id', 'cols', 'rows'] as const,
    invoke: (id: string, cols: number, rows: number) => invoke<void>('pty_resize', { id, cols, rows }),
  },

  pty_detach: {
    name: 'pty_detach' as const,
    params: ['id'] as const,
    invoke: (id: string) => invoke<void>('pty_detach', { id }),
  },

  pty_kill: {
    name: 'pty_kill' as const,
    params: ['id'] as const,
    invoke: (id: string) => invoke<void>('pty_kill', { id }),
  },

  pty_list_sessions: {
    name: 'pty_list_sessions' as const,
    params: [] as const,
    invoke: () => invoke<string[]>('pty_list_sessions', {}),
  },

  get_agent_presets: {
    name: 'get_agent_presets' as const,
    params: [] as const,
    invoke: () => invoke<AgentPreset[]>('get_agent_presets', {}),
  },

  set_agent_presets: {
    name: 'set_agent_presets' as const,
    params: ['presets'] as const,
    invoke: (presets: AgentPreset[]) => invoke<AgentPreset[]>('set_agent_presets', { presets }),
  },
} as const

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface PtyDataPayload { id: string; data: string }
interface PtyExitPayload { id: string; exitCode: number }

export const EVENTS = {
  'pty:data': {
    name: 'pty:data' as const,
    listen: (cb: (id: string, data: string) => void): Promise<UnlistenFn> =>
      listen<PtyDataPayload>('pty:data', (e) => cb(e.payload.id, e.payload.data)),
  },
  'pty:exit': {
    name: 'pty:exit' as const,
    listen: (cb: (id: string, exitCode: number) => void): Promise<UnlistenFn> =>
      listen<PtyExitPayload>('pty:exit', (e) => cb(e.payload.id, e.payload.exitCode)),
  },
} as const

export type CommandName = (typeof COMMANDS)[keyof typeof COMMANDS]['name']
export type EventName = (typeof EVENTS)[keyof typeof EVENTS]['name']
