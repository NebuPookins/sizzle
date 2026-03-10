import fs from 'fs'
import path from 'path'

const MANIFEST_FILES = new Set([
  'Cargo.toml',
  'package.json',
  'go.mod',
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
  '.lua', '.php', '.r', '.jl', '.asm', '.z80',
])

export function isProjectRoot(dir: string): boolean {
  let entries: string[]
  try {
    entries = fs.readdirSync(dir)
  } catch {
    return false
  }

  // Has .git directory
  if (entries.includes('.git')) return true

  // Has README
  if (entries.some((e) => e.toLowerCase().startsWith('readme'))) return true

  // Has a known manifest file
  if (entries.some((e) => MANIFEST_FILES.has(e))) return true

  // Has >= 3 source files
  const sourceCount = entries.filter((e) => {
    const ext = path.extname(e).toLowerCase()
    return SOURCE_EXTENSIONS.has(ext)
  }).length

  return sourceCount >= 3
}
