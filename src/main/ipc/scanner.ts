import { ipcMain } from 'electron'
import fs from 'fs'
import path from 'path'
import { scanForProjects } from '../scanner'

const ROOT_DIR = '/mnt/safe/home/nebu/myPrograms'
const MAX_TEXT_PREVIEW_BYTES = 2 * 1024 * 1024
const MAX_MEDIA_PREVIEW_BYTES = 30 * 1024 * 1024

type PreviewKind = 'text' | 'media' | 'unsupported' | 'tooLarge' | 'error'

export interface FileSystemEntry {
  name: string
  path: string
  isDirectory: boolean
}

export interface FilePreview {
  kind: PreviewKind
  content?: string
  mimeType?: string
  size?: number
  message?: string
}

const TEXT_EXTENSIONS = new Set([
  '.md', '.markdown', '.txt', '.rst', '.json', '.jsonc', '.yml', '.yaml', '.toml', '.ini',
  '.conf', '.config', '.xml', '.csv', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs', '.css',
  '.scss', '.sass', '.less', '.html', '.htm', '.sh', '.bash', '.zsh', '.fish', '.env',
  '.gitignore', '.gitattributes', '.npmrc', '.editorconfig', '.py', '.java', '.go', '.rs',
  '.c', '.cc', '.cpp', '.h', '.hpp', '.sql', '.graphql', '.proto', '.log', '.lock',
])

const MEDIA_MIME_BY_EXTENSION: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.bmp': 'image/bmp',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.ogg': 'video/ogg',
  '.mov': 'video/quicktime',
  '.m4v': 'video/x-m4v',
  '.mp3': 'audio/mpeg',
  '.wav': 'audio/wav',
  '.flac': 'audio/flac',
  '.m4a': 'audio/mp4',
  '.aac': 'audio/aac',
  '.oga': 'audio/ogg',
  '.opus': 'audio/opus',
  '.pdf': 'application/pdf',
}

function normalizePath(inputPath: string): string {
  return path.resolve(inputPath)
}

function isWithinRoot(rootPath: string, candidatePath: string): boolean {
  const normalizedRoot = normalizePath(rootPath)
  const normalizedCandidate = normalizePath(candidatePath)
  return normalizedCandidate === normalizedRoot || normalizedCandidate.startsWith(`${normalizedRoot}${path.sep}`)
}

function isLikelyTextBuffer(buffer: Buffer): boolean {
  const probeLength = Math.min(buffer.length, 4096)
  for (let i = 0; i < probeLength; i += 1) {
    if (buffer[i] === 0) return false
  }
  return true
}

export function registerScannerHandlers(): void {
  ipcMain.handle('scanner:scan', async () => {
    return await scanForProjects(ROOT_DIR)
  })

  ipcMain.handle('markdown:getFiles', async (_event, projectPath: string) => {
    try {
      const entries = fs.readdirSync(projectPath)
      return entries
        .filter((e) => {
          const lower = e.toLowerCase()
          return lower.endsWith('.md') || lower.endsWith('.txt') || lower.endsWith('.rst')
        })
        .sort((a, b) => {
          // README first
          const aIsReadme = a.toLowerCase().startsWith('readme')
          const bIsReadme = b.toLowerCase().startsWith('readme')
          if (aIsReadme && !bIsReadme) return -1
          if (!aIsReadme && bIsReadme) return 1
          return a.localeCompare(b)
        })
        .map((e) => path.join(projectPath, e))
    } catch {
      return []
    }
  })

  ipcMain.handle('markdown:readFile', async (_event, filePath: string) => {
    try {
      return fs.readFileSync(filePath, 'utf-8')
    } catch {
      return null
    }
  })

  ipcMain.handle('files:listDirectory', async (_event, projectPath: string, directoryPath?: string) => {
    try {
      const rootPath = normalizePath(projectPath)
      const targetDirectory = normalizePath(directoryPath ?? projectPath)
      if (!isWithinRoot(rootPath, targetDirectory)) return []

      const stat = fs.statSync(targetDirectory)
      if (!stat.isDirectory()) return []

      const entries = fs.readdirSync(targetDirectory, { withFileTypes: true })
      return entries
        .map((entry): FileSystemEntry => ({
          name: entry.name,
          path: path.join(targetDirectory, entry.name),
          isDirectory: entry.isDirectory(),
        }))
        .sort((a, b) => {
          if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
          return a.name.localeCompare(b.name)
        })
    } catch {
      return []
    }
  })

  ipcMain.handle('files:preview', async (_event, projectPath: string, filePath: string): Promise<FilePreview> => {
    try {
      const rootPath = normalizePath(projectPath)
      const normalizedFilePath = normalizePath(filePath)
      if (!isWithinRoot(rootPath, normalizedFilePath)) {
        return { kind: 'error', message: 'Path is outside project root.' }
      }

      const stat = fs.statSync(normalizedFilePath)
      if (!stat.isFile()) return { kind: 'unsupported', message: 'Not a file.' }

      const ext = path.extname(normalizedFilePath).toLowerCase()
      const knownMime = MEDIA_MIME_BY_EXTENSION[ext]
      if (knownMime) {
        if (stat.size > MAX_MEDIA_PREVIEW_BYTES) {
          return { kind: 'tooLarge', size: stat.size, message: 'File is too large for media preview.' }
        }
        const fileBuffer = fs.readFileSync(normalizedFilePath)
        return {
          kind: 'media',
          content: fileBuffer.toString('base64'),
          mimeType: knownMime,
          size: stat.size,
        }
      }

      if (TEXT_EXTENSIONS.has(ext)) {
        if (stat.size > MAX_TEXT_PREVIEW_BYTES) {
          return { kind: 'tooLarge', size: stat.size, message: 'File is too large for text preview.' }
        }
        return { kind: 'text', content: fs.readFileSync(normalizedFilePath, 'utf-8'), size: stat.size }
      }

      if (stat.size <= MAX_TEXT_PREVIEW_BYTES) {
        const fileBuffer = fs.readFileSync(normalizedFilePath)
        if (isLikelyTextBuffer(fileBuffer)) {
          return { kind: 'text', content: fileBuffer.toString('utf-8'), size: stat.size }
        }
      }

      return { kind: 'unsupported', size: stat.size, message: 'Unsupported file format.' }
    } catch {
      return { kind: 'error', message: 'Failed to load file preview.' }
    }
  })
}
