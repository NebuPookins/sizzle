import fs from 'fs'
import path from 'path'
import os from 'os'

const CONFIG_DIR = path.join(os.homedir(), '.config', 'sizzle')
const DB_PATH = path.join(CONFIG_DIR, 'db.json')
const TMP_PATH = path.join(CONFIG_DIR, 'db.json.tmp')

interface ProjectMeta {
  lastLaunched: number | null
  tagOverride: ProjectTagOverride | null
}

export interface ProjectTag {
  name: string
  score: number
}

export interface ProjectTagOverride {
  tags: ProjectTag[]
  primaryTag: string | null
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

function pruneMissingProjects(db: DB): boolean {
  let changed = false
  for (const projectPath of Object.keys(db.projects)) {
    if (fs.existsSync(projectPath)) continue
    delete db.projects[projectPath]
    changed = true
  }
  return changed
}

export function getMetadata(projectPath: string): ProjectMeta {
  const db = readDB()
  return db.projects[projectPath] ?? { lastLaunched: null, tagOverride: null }
}

export function setLastLaunched(projectPath: string): void {
  const db = readDB()
  if (!db.projects[projectPath]) {
    db.projects[projectPath] = { lastLaunched: null, tagOverride: null }
  }
  db.projects[projectPath].lastLaunched = Date.now()
  writeDB(db)
}

export function getAllMetadata(): Record<string, ProjectMeta> {
  const db = readDB()
  const prunedMissingProjects = pruneMissingProjects(db)
  let normalized = prunedMissingProjects
  for (const projectPath of Object.keys(db.projects)) {
    if (!db.projects[projectPath].tagOverride) {
      db.projects[projectPath].tagOverride = null
      normalized = true
    }
  }
  if (normalized) {
    writeDB(db)
  }
  return db.projects
}

function sanitizeTagName(value: string): string {
  return value.trim().replace(/\s+/g, ' ')
}

function normalizeTagOverride(value: ProjectTagOverride): ProjectTagOverride {
  const tagMap = new Map<string, number>()
  for (const tag of value.tags) {
    if (!tag || typeof tag.name !== 'string') continue
    const name = sanitizeTagName(tag.name)
    if (!name) continue
    const score = Number.isFinite(tag.score) && tag.score > 0 ? tag.score : 0
    tagMap.set(name, Math.max(tagMap.get(name) ?? 0, score))
  }

  const tags = Array.from(tagMap.entries())
    .map(([name, score]) => ({ name, score }))
    .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))

  const totalScore = tags.reduce((sum, tag) => sum + tag.score, 0)
  const normalizedTags = totalScore > 0
    ? tags.map((tag) => ({ ...tag, score: tag.score / totalScore }))
    : tags.map((tag) => ({ ...tag, score: 1 / Math.max(tags.length, 1) }))

  const primaryTag = typeof value.primaryTag === 'string' && value.primaryTag.trim()
    ? sanitizeTagName(value.primaryTag)
    : null
  const resolvedPrimary = primaryTag && normalizedTags.some((tag) => tag.name === primaryTag)
    ? primaryTag
    : (normalizedTags[0]?.name ?? null)

  return {
    tags: normalizedTags,
    primaryTag: resolvedPrimary,
  }
}

export function setTagOverride(projectPath: string, override: ProjectTagOverride | null): ProjectMeta {
  const db = readDB()
  if (!db.projects[projectPath]) {
    db.projects[projectPath] = { lastLaunched: null, tagOverride: null }
  }
  db.projects[projectPath].tagOverride = override ? normalizeTagOverride(override) : null
  writeDB(db)
  return db.projects[projectPath]
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
