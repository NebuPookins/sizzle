import type { ScannedProject, ProjectMeta, FileSystemEntry, FilePreview, ScanSettings } from '../preload/index'

declare global {
  interface Window {
    sizzle: {
      defaultShell: string
      scanProjects(): Promise<ScannedProject[]>
      getScanSettings(): Promise<ScanSettings>
      setScanSettings(settings: ScanSettings): Promise<ScanSettings>
      addIgnoreRoot(rootPath: string): Promise<ScanSettings>
      pickDirectory(): Promise<string | null>
      getMarkdownFiles(projectPath: string): Promise<string[]>
      readMarkdownFile(filePath: string): Promise<string | null>
      listDirectory(projectPath: string, directoryPath?: string): Promise<FileSystemEntry[]>
      previewFile(projectPath: string, filePath: string): Promise<FilePreview>
      getMetadata(projectPath: string): Promise<ProjectMeta>
      getAllMetadata(): Promise<Record<string, ProjectMeta>>
      setLastLaunched(projectPath: string): Promise<void>
      ptyCreate(id: string, cwd: string, command: string, args: string[]): Promise<void>
      ptyWrite(id: string, data: string): Promise<void>
      ptyResize(id: string, cols: number, rows: number): Promise<void>
      ptyKill(id: string): Promise<void>
      onPtyData(callback: (id: string, data: string) => void): () => void
      onPtyExit(callback: (id: string, exitCode: number) => void): () => void
    }
  }
}

export {}
