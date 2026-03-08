import fs from 'fs'
import path from 'path'
import os from 'os'

const CONFIG_DIR = path.join(os.homedir(), '.config', 'sizzle')
const DB_PATH = path.join(CONFIG_DIR, 'db.json')
const TMP_PATH = path.join(CONFIG_DIR, 'db.json.tmp')

interface ProjectMeta {
  lastLaunched: number | null
}

interface DB {
  projects: Record<string, ProjectMeta>
}

let cache: DB | null = null

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
