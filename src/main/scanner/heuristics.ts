import fs from 'fs'
import path from 'path'

const MANIFEST_FILES = new Set([
  'Cargo.toml',
  'package.json',
  'go.mod',
  'flex-config.xml',
  'air-app.xml',
  'pyproject.toml',
  'setup.py',
  'pom.xml',
  'build.gradle',
  'Makefile',
  'CMakeLists.txt',
  'meson.build',
  'mix.exs',
  'composer.json',
  'Gemfile',
  'pubspec.yaml',
  'build.sbt',
  'project.clj',
  'deps.edn',
])

const SOURCE_EXTENSIONS = new Set([
  '.rs', '.ts', '.tsx', '.js', '.jsx', '.py', '.java', '.kt', '.go',
  '.c', '.cpp', '.cc', '.cxx', '.h', '.hpp', '.cs', '.rb', '.swift',
  '.scala', '.clj', '.ex', '.exs', '.hs', '.ml', '.elm', '.dart',
  '.lua', '.php', '.r', '.jl', '.asm', '.z80', '.as', '.mxml',
])

const WEB_ENTRY_FILES = new Set([
  'index.html',
  'index.htm',
])

const WEB_ASSET_EXTENSIONS = new Set([
  '.js', '.jsx', '.ts', '.tsx', '.css', '.scss', '.sass', '.less',
])

function isDocumentationMarker(entryName: string): boolean {
  const lower = entryName.toLowerCase()
  return lower.startsWith('readme') || lower === 'agents.md'
}

export function isProjectRoot(dir: string): boolean {
  let entries: string[]
  try {
    entries = fs.readdirSync(dir)
  } catch {
    return false
  }

  // Has .git directory
  if (entries.includes('.git')) return true

  // Has README or AGENTS.md
  if (entries.some(isDocumentationMarker)) return true

  // Has a known manifest file
  if (entries.some((e) => MANIFEST_FILES.has(e))) return true

  // Simple static web app: HTML entry point plus at least one companion asset file
  if (
    entries.some((entry) => WEB_ENTRY_FILES.has(entry.toLowerCase()))
    && entries.some((entry) => WEB_ASSET_EXTENSIONS.has(path.extname(entry).toLowerCase()))
  ) {
    return true
  }

  // Has >= 3 source files
  const sourceCount = entries.filter((e) => {
    const ext = path.extname(e).toLowerCase()
    return SOURCE_EXTENSIONS.has(ext)
  }).length

  return sourceCount >= 3
}
