import fs from 'fs'
import path from 'path'
import os from 'os'

const CONFIG_DIR = path.join(os.homedir(), '.config', 'sizzle')
const DB_PATH = path.join(CONFIG_DIR, 'db.json')
const TMP_PATH = path.join(CONFIG_DIR, 'db.json.tmp')

interface ProjectMeta {
  lastLaunched: number | null
}

export interface ScanSettings {
  scanRoots: string[]
  ignoreRoots: string[]
  manualProjectRoots: string[]
}

interface DB {
  projects: Record<string, ProjectMeta>
  scanSettings?: ScanSettings
}

let cache: DB | null = null
const DEFAULT_SCAN_ROOT = '/mnt/safe/home/nebu/myPrograms'

function normalizeRootPath(rootPath: string): string {
  return path.resolve(rootPath.trim())
}

function sanitizeRootList(values: unknown): string[] {
  if (!Array.isArray(values)) return []
  const unique = new Set<string>()
  for (const value of values) {
    if (typeof value !== 'string') continue
    if (!value.trim()) continue
    const normalized = normalizeRootPath(value)
    if (normalized) unique.add(normalized)
  }
  return Array.from(unique)
}

function getNormalizedScanSettings(db: DB): ScanSettings {
  const scanRoots = sanitizeRootList(db.scanSettings?.scanRoots)
  const ignoreRoots = sanitizeRootList(db.scanSettings?.ignoreRoots)
  const manualProjectRoots = sanitizeRootList(db.scanSettings?.manualProjectRoots)
  return {
    scanRoots: scanRoots.length > 0 ? scanRoots : [DEFAULT_SCAN_ROOT],
    ignoreRoots,
    manualProjectRoots,
  }
}

function ensureDir(): void {
  if (!fs.existsSync(CONFIG_DIR)) {
    fs.mkdirSync(CONFIG_DIR, { recursive: true })
  }
}

function readDB(): DB {
  if (cache) return cache
  ensureDir()
  try {
    const raw = fs.readFileSync(DB_PATH, 'utf-8')
    cache = JSON.parse(raw) as DB
  } catch {
    cache = { projects: {} }
  }
  return cache
}

function writeDB(db: DB): void {
  ensureDir()
  fs.writeFileSync(TMP_PATH, JSON.stringify(db, null, 2), 'utf-8')
  fs.renameSync(TMP_PATH, DB_PATH)
  cache = db
}

export function getMetadata(projectPath: string): ProjectMeta {
  const db = readDB()
  return db.projects[projectPath] ?? { lastLaunched: null }
}

export function setLastLaunched(projectPath: string): void {
  const db = readDB()
  if (!db.projects[projectPath]) {
    db.projects[projectPath] = { lastLaunched: null }
  }
  db.projects[projectPath].lastLaunched = Date.now()
  writeDB(db)
}

export function getAllMetadata(): Record<string, ProjectMeta> {
  return readDB().projects
}

export function getScanSettings(): ScanSettings {
  const db = readDB()
  const normalized = getNormalizedScanSettings(db)
  if (
    !db.scanSettings
    || db.scanSettings.scanRoots?.length !== normalized.scanRoots.length
    || db.scanSettings.ignoreRoots?.length !== normalized.ignoreRoots.length
    || db.scanSettings.manualProjectRoots?.length !== normalized.manualProjectRoots.length
    || db.scanSettings.scanRoots?.some((value, index) => value !== normalized.scanRoots[index])
    || db.scanSettings.ignoreRoots?.some((value, index) => value !== normalized.ignoreRoots[index])
    || db.scanSettings.manualProjectRoots?.some((value, index) => value !== normalized.manualProjectRoots[index])
  ) {
    db.scanSettings = normalized
    writeDB(db)
  }
  return normalized
}

export function setScanSettings(settings: ScanSettings): ScanSettings {
  const db = readDB()
  const normalized: ScanSettings = {
    scanRoots: sanitizeRootList(settings.scanRoots),
    ignoreRoots: sanitizeRootList(settings.ignoreRoots),
    manualProjectRoots: sanitizeRootList(settings.manualProjectRoots),
  }
  if (normalized.scanRoots.length === 0) {
    normalized.scanRoots = [DEFAULT_SCAN_ROOT]
  }
  db.scanSettings = normalized
  writeDB(db)
  return normalized
}
