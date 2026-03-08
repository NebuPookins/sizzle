import fs from 'fs'
import path from 'path'
import { isProjectRoot } from './heuristics'

const SKIP_DIRS = new Set([
  'node_modules', '.git', 'target', 'dist', 'build', '__pycache__',
  'vendor', 'venv', '.venv', '.cache', '.npm', '.cargo', 'out',
])

export interface ScannedProject {
  name: string
  path: string
  readmeFiles: string[]
}

function isPathWithinRoot(rootPath: string, candidatePath: string): boolean {
  const normalizedRoot = path.resolve(rootPath)
  const normalizedCandidate = path.resolve(candidatePath)
  return normalizedCandidate === normalizedRoot || normalizedCandidate.startsWith(`${normalizedRoot}${path.sep}`)
}

function shouldSkipByIgnoreRoots(dir: string, ignoreRoots: string[]): boolean {
  return ignoreRoots.some((ignoreRoot) => isPathWithinRoot(ignoreRoot, dir))
}

async function scanDir(dir: string, ignoreRoots: string[], results: ScannedProject[]): Promise<void> {
  if (shouldSkipByIgnoreRoots(dir, ignoreRoots)) return

  let entries: fs.Dirent[]
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true })
  } catch {
    return
  }

  if (isProjectRoot(dir)) {
    const readmeFiles = entries
      .filter((e) => e.isFile() && e.name.toLowerCase().startsWith('readme'))
      .map((e) => path.join(dir, e.name))

    results.push({
      name: path.basename(dir),
      path: dir,
      readmeFiles,
    })
    return // Don't recurse into project roots
  }

  const subdirs = entries.filter(
    (e) => e.isDirectory() && !SKIP_DIRS.has(e.name) && !e.name.startsWith('.')
  )

  await Promise.all(
    subdirs.map((e) => scanDir(path.join(dir, e.name), ignoreRoots, results))
  )
}

export async function scanForProjects(rootDirs: string[], ignoreRoots: string[]): Promise<ScannedProject[]> {
  const results: ScannedProject[] = []
  await Promise.all(rootDirs.map((rootDir) => scanDir(rootDir, ignoreRoots, results)))
  return results
}
