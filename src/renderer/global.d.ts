import type {
  ScannedProject,
  ProjectTag,
  ProjectMeta,
  ProjectMarker,
  ProjectTagOverride,
  FileSystemEntry,
  ArchiveTreeNode,
  FilePreview,
  ProjectRepositoryInfo,
  GitStatus,
  PtyOpenResult,
  ScanSettings,
} from '../preload/index'
import type { ReloadSnapshot } from '../shared/reload'

declare global {
  interface Window {
    sizzle: {
      defaultShell: string
      scanProjects(): Promise<ScannedProject[]>
      rescanProjectTags(projectPath: string): Promise<ProjectTag[]>
      getScanSettings(): Promise<ScanSettings>
      setScanSettings(settings: ScanSettings): Promise<ScanSettings>
      addIgnoreRoot(rootPath: string): Promise<ScanSettings>
      pickDirectory(): Promise<string | null>
      getMarkdownFiles(projectPath: string): Promise<string[]>
      readMarkdownFile(filePath: string): Promise<string | null>
      getProjectRepositoryInfo(projectPath: string): Promise<ProjectRepositoryInfo>
      getGitStatus(projectPath: string): Promise<GitStatus | null>
      listDirectory(projectPath: string, directoryPath?: string): Promise<FileSystemEntry[]>
      previewFile(projectPath: string, filePath: string): Promise<FilePreview>
      getMetadata(projectPath: string): Promise<ProjectMeta>
      getAllMetadata(): Promise<Record<string, ProjectMeta>>
      setLastLaunched(projectPath: string): Promise<void>
      setTagOverride(projectPath: string, override: ProjectTagOverride | null): Promise<ProjectMeta>
      setProjectMarker(projectPath: string, marker: ProjectMarker): Promise<ProjectMeta>
      consumeReloadSnapshot(): Promise<ReloadSnapshot | null>
      reloadCore(snapshot: ReloadSnapshot): Promise<void>
      ptyCreate(id: string, cwd: string, command: string, args: string[]): Promise<PtyOpenResult>
      ptyWrite(id: string, data: string): void
      ptyResize(id: string, cols: number, rows: number): Promise<void>
      ptyDetach(id: string): Promise<void>
      ptyKill(id: string): Promise<void>
      onPtyData(callback: (id: string, data: string) => void): () => void
      onPtyExit(callback: (id: string, exitCode: number) => void): () => void
      claudeHasSession(projectPath: string): Promise<boolean>
    }
  }
}

export {}
