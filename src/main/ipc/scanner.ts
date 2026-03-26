import { dialog, ipcMain } from 'electron'
import fs from 'fs'
import path from 'path'
import { execFile } from 'child_process'
import { promisify } from 'util'
import yauzl from 'yauzl'
import { scanForProjects } from '../scanner'
import { detectProjectTags } from '../scanner/tags'
import { type ScanSettings, getScanSettings, setScanSettings } from '../store/metadata'

const execFileAsync = promisify(execFile)

const MAX_TEXT_PREVIEW_BYTES = 2 * 1024 * 1024
const MAX_MEDIA_PREVIEW_BYTES = 30 * 1024 * 1024

type PreviewKind = 'text' | 'media' | 'archive' | 'unsupported' | 'tooLarge' | 'error'

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

export interface FilePreview {
  kind: PreviewKind
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

function parseGitStatus(stdout: string): GitStatus {
  const lines = stdout.split('\n')
  let branch: string | null = null
  let upstream: string | null = null
  let ahead = 0
  let behind = 0
  let isDetached = false
  const staged: GitFileChange[] = []
  const unstaged: GitFileChange[] = []
  const untracked: string[] = []

  for (const line of lines) {
    if (line.startsWith('## ')) {
      const branchLine = line.slice(3)
      if (branchLine.startsWith('HEAD (no branch)')) {
        isDetached = true
        continue
      }
      const noCommitsMatch = branchLine.match(/^No commits yet on (.+)$/)
      if (noCommitsMatch) {
        branch = noCommitsMatch[1].trim()
        continue
      }
      const dotDotDotIdx = branchLine.indexOf('...')
      if (dotDotDotIdx === -1) {
        branch = branchLine
      } else {
        branch = branchLine.slice(0, dotDotDotIdx)
        const rest = branchLine.slice(dotDotDotIdx + 3)
        const bracketIdx = rest.indexOf(' [')
        upstream = bracketIdx === -1 ? rest : rest.slice(0, bracketIdx)
        if (bracketIdx !== -1) {
          const bracketContent = rest.slice(bracketIdx + 2, rest.lastIndexOf(']'))
          const aheadMatch = bracketContent.match(/ahead (\d+)/)
          const behindMatch = bracketContent.match(/behind (\d+)/)
          if (aheadMatch) ahead = parseInt(aheadMatch[1], 10)
          if (behindMatch) behind = parseInt(behindMatch[1], 10)
        }
      }
      continue
    }

    if (line.length < 3) continue
    const x = line[0]
    const y = line[1]
    const rawPath = line.slice(3)

    if (x === '?' && y === '?') {
      untracked.push(rawPath)
      continue
    }
    if (x === '!' && y === '!') continue

    if (x !== ' ') {
      let filePath = rawPath
      let origPath: string | undefined
      if (x === 'R' || x === 'C') {
        const tabIdx = rawPath.indexOf('\t')
        const arrowIdx = rawPath.indexOf(' -> ')
        if (tabIdx !== -1) {
          origPath = rawPath.slice(0, tabIdx)
          filePath = rawPath.slice(tabIdx + 1)
        } else if (arrowIdx !== -1) {
          origPath = rawPath.slice(0, arrowIdx)
          filePath = rawPath.slice(arrowIdx + 4)
        }
      }
      staged.push({ status: x, path: filePath, origPath })
    }

    if (y !== ' ') {
      const tabIdx = rawPath.indexOf('\t')
      unstaged.push({ status: y, path: tabIdx !== -1 ? rawPath.slice(0, tabIdx) : rawPath })
    }
  }

  return { branch, upstream, ahead, behind, staged, unstaged, untracked, isDetached }
}

async function fetchGitStatus(projectPath: string): Promise<GitStatus | null> {
  try {
    const { stdout } = await execFileAsync('git', ['status', '--porcelain', '-b'], {
      cwd: projectPath,
      env: process.env,
      maxBuffer: 512 * 1024,
    })
    return parseGitStatus(stdout)
  } catch {
    return null
  }
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

async function ensureScanSettingsConfigured(): Promise<ScanSettings> {
  const settings = getScanSettings()
  if (settings.scanRoots.length > 0) return settings

  const result = await dialog.showOpenDialog({
    title: 'Choose a projects root folder',
    message: 'Select the directory Sizzle should scan for projects.',
    buttonLabel: 'Use this folder',
    properties: ['openDirectory', 'createDirectory'],
  })

  if (result.canceled || result.filePaths.length === 0) {
    return settings
  }

  return setScanSettings({
    ...settings,
    scanRoots: [result.filePaths[0]],
  })
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

function normalizeArchiveEntryPath(entryPath: string): string {
  return entryPath
    .replace(/\\/g, '/')
    .split('/')
    .filter((segment) => segment && segment !== '.' && segment !== '..')
    .join('/')
}

function sortArchiveTree(nodes: ArchiveTreeNode[]): void {
  nodes.sort((a, b) => {
    if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
    return a.name.localeCompare(b.name)
  })

  for (const node of nodes) {
    if (node.children) sortArchiveTree(node.children)
  }
}

function readZipArchiveTree(filePath: string): Promise<ArchiveTreeNode[]> {
  return new Promise((resolve, reject) => {
    yauzl.open(filePath, { lazyEntries: true }, (openError, zipFile) => {
      if (openError || !zipFile) {
        reject(openError ?? new Error('Failed to open zip file.'))
        return
      }

      const roots: ArchiveTreeNode[] = []
      const nodeByPath = new Map<string, ArchiveTreeNode>()

      function ensureNode(nodePath: string, isDirectory: boolean): ArchiveTreeNode {
        const existing = nodeByPath.get(nodePath)
        if (existing) {
          if (isDirectory && !existing.isDirectory) {
            existing.isDirectory = true
            existing.children = existing.children ?? []
          }
          return existing
        }

        const name = nodePath.split('/').pop() ?? nodePath
        const node: ArchiveTreeNode = {
          name,
          path: nodePath,
          isDirectory,
          children: isDirectory ? [] : undefined,
        }
        nodeByPath.set(nodePath, node)

        const slashIndex = nodePath.lastIndexOf('/')
        if (slashIndex === -1) {
          roots.push(node)
        } else {
          const parentPath = nodePath.slice(0, slashIndex)
          const parent = ensureNode(parentPath, true)
          parent.children = parent.children ?? []
          parent.children.push(node)
        }

        return node
      }

      zipFile.on('entry', (entry) => {
        const normalizedPath = normalizeArchiveEntryPath(entry.fileName)
        if (!normalizedPath) {
          zipFile.readEntry()
          return
        }

        const isDirectory = entry.fileName.endsWith('/')
        const segments = normalizedPath.split('/')
        let currentPath = ''
        for (let index = 0; index < segments.length; index += 1) {
          currentPath = currentPath ? `${currentPath}/${segments[index]}` : segments[index]
          ensureNode(currentPath, isDirectory || index < segments.length - 1)
        }

        zipFile.readEntry()
      })

      zipFile.once('end', () => {
        sortArchiveTree(roots)
        zipFile.close()
        resolve(roots)
      })

      zipFile.once('error', (error) => {
        zipFile.close()
        reject(error)
      })

      zipFile.readEntry()
    })
  })
}

function findGitDirectory(startPath: string): string | null {
  let currentPath = normalizePath(startPath)

  while (true) {
    const dotGitPath = path.join(currentPath, '.git')
    if (fs.existsSync(dotGitPath)) {
      const stat = fs.statSync(dotGitPath)
      if (stat.isDirectory()) return dotGitPath
      if (stat.isFile()) {
        try {
          const raw = fs.readFileSync(dotGitPath, 'utf-8')
          const match = raw.match(/gitdir:\s*(.+)/i)
          if (!match) return null
          const gitDir = match[1].trim()
          return path.resolve(currentPath, gitDir)
        } catch {
          return null
        }
      }
    }

    const parentPath = path.dirname(currentPath)
    if (parentPath === currentPath) return null
    currentPath = parentPath
  }
}

function parseGitConfig(configContent: string): Record<string, Record<string, string>> {
  const sections: Record<string, Record<string, string>> = {}
  let currentSection: string | null = null

  for (const rawLine of configContent.split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line || line.startsWith('#') || line.startsWith(';')) continue

    const sectionMatch = line.match(/^\[(.+)]$/)
    if (sectionMatch) {
      currentSection = sectionMatch[1].trim()
      if (!sections[currentSection]) sections[currentSection] = {}
      continue
    }

    if (!currentSection) continue
    const separatorIndex = line.indexOf('=')
    if (separatorIndex === -1) continue

    const key = line.slice(0, separatorIndex).trim()
    const value = line.slice(separatorIndex + 1).trim()
    sections[currentSection][key] = value
  }

  return sections
}

function normalizeGitHubRemote(remoteUrl: string): string | null {
  const trimmed = remoteUrl.trim()
  if (!trimmed) return null

  const sshMatch = trimmed.match(/^git@github\.com:(.+?)(?:\.git)?$/i)
  if (sshMatch) return `https://github.com/${sshMatch[1]}`

  const httpsMatch = trimmed.match(/^https?:\/\/github\.com\/(.+?)(?:\.git)?(?:\/)?$/i)
  if (httpsMatch) return `https://github.com/${httpsMatch[1]}`

  const sshProtocolMatch = trimmed.match(/^ssh:\/\/git@github\.com\/(.+?)(?:\.git)?(?:\/)?$/i)
  if (sshProtocolMatch) return `https://github.com/${sshProtocolMatch[1]}`

  return null
}

function getProjectRepositoryInfo(projectPath: string): ProjectRepositoryInfo {
  try {
    const gitDirectory = findGitDirectory(projectPath)
    if (!gitDirectory) return { isGitRepo: false, githubUrl: null }

    const configPath = path.join(gitDirectory, 'config')
    const headPath = path.join(gitDirectory, 'HEAD')
    if (!fs.existsSync(configPath)) return { isGitRepo: true, githubUrl: null }

    const config = parseGitConfig(fs.readFileSync(configPath, 'utf-8'))
    const headContent = fs.existsSync(headPath) ? fs.readFileSync(headPath, 'utf-8').trim() : ''
    const headMatch = headContent.match(/^ref:\s+refs\/heads\/(.+)$/)
    const currentBranch = headMatch?.[1] ?? null
    const branchRemoteName = currentBranch ? config[`branch "${currentBranch}"`]?.remote : null

    const remoteCandidates = [
      branchRemoteName,
      'origin',
      ...Object.keys(config)
        .map((key) => key.match(/^remote "(.+)"$/)?.[1] ?? null)
        .filter((value): value is string => value !== null),
    ]

    const seen = new Set<string>()
    for (const remoteName of remoteCandidates) {
      if (!remoteName || seen.has(remoteName)) continue
      seen.add(remoteName)
      const remoteUrl = config[`remote "${remoteName}"`]?.url
      if (!remoteUrl) continue
      const githubUrl = normalizeGitHubRemote(remoteUrl)
      if (githubUrl) return { isGitRepo: true, githubUrl }
    }

    return { isGitRepo: true, githubUrl: null }
  } catch {
    return { isGitRepo: false, githubUrl: null }
  }
}

export function registerScannerHandlers(): void {
  ipcMain.handle('scanner:scan', async () => {
    const settings = await ensureScanSettingsConfigured()
    if (settings.scanRoots.length === 0) return []
    const scanned = await scanForProjects(
      settings.scanRoots,
      settings.ignoreRoots,
      settings.manualProjectRoots,
    )
    const deduped = new Map<string, (typeof scanned)[number]>()
    for (const project of scanned) {
      if (!deduped.has(project.path)) {
        deduped.set(project.path, project)
      }
    }
    return Array.from(deduped.values())
  })

  ipcMain.handle('scanner:rescanProject', async (_event, projectPath: string) => {
    return detectProjectTags(projectPath)
  })

  ipcMain.handle('scanner:getSettings', async () => {
    return getScanSettings()
  })

  ipcMain.handle(
    'scanner:setSettings',
    async (
      _event,
      settings: { scanRoots: string[]; ignoreRoots: string[]; manualProjectRoots: string[] },
    ) => {
      return setScanSettings(settings)
    },
  )

  ipcMain.handle('scanner:addIgnoreRoot', async (_event, rootPath: string) => {
    const current = getScanSettings()
    const nextIgnoreRoots = [...current.ignoreRoots, rootPath]
    return setScanSettings({
      scanRoots: current.scanRoots,
      ignoreRoots: nextIgnoreRoots,
      manualProjectRoots: current.manualProjectRoots,
    })
  })

  ipcMain.handle('scanner:pickDirectory', async () => {
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory', 'createDirectory'],
    })
    if (result.canceled || result.filePaths.length === 0) return null
    return result.filePaths[0]
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

  ipcMain.handle('project:getRepositoryInfo', async (_event, projectPath: string) => {
    return getProjectRepositoryInfo(projectPath)
  })

  ipcMain.handle('git:getStatus', async (_event, projectPath: string) => {
    return fetchGitStatus(projectPath)
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
      if (ext === '.zip') {
        return {
          kind: 'archive',
          size: stat.size,
          archiveTree: await readZipArchiveTree(normalizedFilePath),
        }
      }

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
