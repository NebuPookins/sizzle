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

async function scanDir(dir: string, results: ScannedProject[]): Promise<void> {
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
    subdirs.map((e) => scanDir(path.join(dir, e.name), results))
  )
}

export async function scanForProjects(rootDir: string): Promise<ScannedProject[]> {
  const results: ScannedProject[] = []
  await scanDir(rootDir, results)
  return results
}
