// All backend calls go through COMMANDS / EVENTS from api-definitions.
// The real invoke() and listen() from @tauri-apps are ONLY imported there.
// This file provides thin typed wrappers for the rest of the app.

import { COMMANDS, EVENTS } from '../shared/api-definitions'
import type {
  ScannedProject,
  ProjectTag,
  ScanSettings,
  FileSystemEntry,
  FilePreview,
  ProjectRepositoryInfo,
  GitStatus,
  ProjectMeta,
  ProjectTagOverride,
  ProjectMarker,
  MoveRenameResult,
  AgentPreset,
  PtyCreateResult,
} from '../shared/api-definitions'

// Re-export types for convenience
export type {
  ScannedProject,
  ProjectTag,
  ProjectTagOverride,
  ProjectMarker,
  ProjectMeta,
  ScanSettings,
  FileSystemEntry,
  FilePreview,
  ProjectRepositoryInfo,
  GitStatus,
  MoveRenameResult,
  AgentPreset,
  PtyCreateResult,
}

// ── Scanner ──

export const scanProjects = (): Promise<ScannedProject[]> =>
  COMMANDS.scan_projects.invoke()

export const rescanProjectTags = (projectPath: string): Promise<ProjectTag[]> =>
  COMMANDS.rescan_project_tags.invoke(projectPath)

export const getScanSettings = (): Promise<ScanSettings> =>
  COMMANDS.get_scan_settings.invoke()

export const setScanSettings = (settings: ScanSettings): Promise<ScanSettings> =>
  COMMANDS.set_scan_settings.invoke(settings)

export const addIgnoreRoot = (rootPath: string): Promise<ScanSettings> =>
  COMMANDS.add_ignore_root.invoke(rootPath)

// ── Agent presets ──

export const getAgentPresets = (): Promise<AgentPreset[]> =>
  COMMANDS.get_agent_presets.invoke()

export const setAgentPresets = (presets: AgentPreset[]): Promise<AgentPreset[]> =>
  COMMANDS.set_agent_presets.invoke(presets)

// ── Files / Markdown ──

export const getMarkdownFiles = (projectPath: string): Promise<string[]> =>
  COMMANDS.get_markdown_files.invoke(projectPath)

export const getProjectDetail = (projectPath: string): Promise<{ markdownFiles: string[]; isGitRepo: boolean; githubUrl: string | null }> =>
  COMMANDS.get_project_detail.invoke(projectPath)

export const readMarkdownFile = (filePath: string): Promise<string | null> =>
  COMMANDS.read_markdown_file.invoke(filePath)

export const listDirectory = (projectPath: string, directoryPath?: string): Promise<FileSystemEntry[]> =>
  COMMANDS.list_directory.invoke(projectPath, directoryPath)

export const previewFile = (projectPath: string, filePath: string): Promise<FilePreview> =>
  COMMANDS.preview_file.invoke(projectPath, filePath)

// ── Git ──

export const getProjectRepositoryInfo = (projectPath: string): Promise<ProjectRepositoryInfo> =>
  COMMANDS.get_project_repository_info.invoke(projectPath)

export const getGitStatus = (projectPath: string): Promise<GitStatus | null> =>
  COMMANDS.get_git_status.invoke(projectPath)

// ── Metadata ──

export const getMetadata = (projectPath: string): Promise<ProjectMeta> =>
  COMMANDS.get_metadata.invoke(projectPath)

export const getAllMetadata = (): Promise<Record<string, ProjectMeta>> =>
  COMMANDS.get_all_metadata.invoke()

export const setLastLaunched = (projectPath: string): Promise<void> =>
  COMMANDS.set_last_launched.invoke(projectPath)

export const setTagOverride = (projectPath: string, override: ProjectTagOverride | null): Promise<ProjectMeta> =>
  COMMANDS.set_tag_override.invoke(projectPath, override)

export const setProjectMarker = (projectPath: string, marker: ProjectMarker): Promise<ProjectMeta> =>
  COMMANDS.set_project_marker.invoke(projectPath, marker)

// ── Claude ──

export const claudeHasSession = (projectPath: string): Promise<boolean> =>
  COMMANDS.claude_has_session.invoke(projectPath)

// ── PTY ──

export const ptyCreate = (id: string, cwd: string, command: string, args: string[]): Promise<PtyCreateResult> =>
  COMMANDS.pty_create.invoke(id, cwd, command, args)

export const ptyWrite = (id: string, data: string): Promise<void> =>
  COMMANDS.pty_write.invoke(id, data)

export const ptyResize = (id: string, cols: number, rows: number): Promise<void> =>
  COMMANDS.pty_resize.invoke(id, cols, rows)

export const ptyDetach = (id: string): Promise<void> =>
  COMMANDS.pty_detach.invoke(id)

export const ptyKill = (id: string): Promise<void> =>
  COMMANDS.pty_kill.invoke(id)

export const ptyListSessions = (): Promise<string[]> =>
  COMMANDS.pty_list_sessions.invoke()

// ── Events ──

export function onPtyData(callback: (id: string, data: string) => void): Promise<import('@tauri-apps/api/event').UnlistenFn> {
  return EVENTS['pty:data'].listen(callback)
}

export function onPtyExit(callback: (id: string, exitCode: number) => void): Promise<import('@tauri-apps/api/event').UnlistenFn> {
  return EVENTS['pty:exit'].listen(callback)
}

// ── Shell / defaults ──

export const getDefaultShell = (): Promise<string> =>
  COMMANDS.get_default_shell.invoke()

export const commandExists = (command: string): Promise<boolean> =>
  COMMANDS.command_exists.invoke(command)

// ── Projects ──

export const moveRenameProject = (oldPath: string, newPath: string): Promise<MoveRenameResult> =>
  COMMANDS.move_rename_project.invoke(oldPath, newPath)

// ── App reload ──

import type { ReloadSnapshot } from '../shared/reload'

export const consumeReloadSnapshot = async (): Promise<ReloadSnapshot | null> => {
  return null
}

export const reloadCore = async (_snapshot: ReloadSnapshot): Promise<void> => {
  throw new Error('Core reload not supported in Tauri build. Relaunch the app instead.')
}

// ── API manifest (sync detection) ──

import type { ApiManifest } from '../shared/api-manifest'

export const getApiManifest = (): Promise<ApiManifest> =>
  COMMANDS.get_api_manifest.invoke()

// ── Window title ──

import { getCurrentWindow } from '@tauri-apps/api/window'

export const setWindowTitle = (projectName: string | null): Promise<void> =>
  getCurrentWindow().setTitle(projectName ? `Sizzle - ${projectName}` : 'Sizzle')
