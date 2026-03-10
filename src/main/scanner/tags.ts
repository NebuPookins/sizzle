import fs from 'fs'
import path from 'path'

export interface ProjectTag {
  name: string
  score: number
}

const SKIP_DIRS = new Set([
  'node_modules', '.git', 'target', 'dist', 'build', '__pycache__',
  'vendor', 'venv', '.venv', '.cache', '.npm', '.cargo', 'out',
])

const MAX_SAMPLED_FILES = 6000

const EXTENSION_LANGUAGES: Record<string, string> = {
  '.ts': 'TypeScript',
  '.tsx': 'TypeScript',
  '.mts': 'TypeScript',
  '.cts': 'TypeScript',
  '.js': 'JavaScript',
  '.jsx': 'JavaScript',
  '.mjs': 'JavaScript',
  '.cjs': 'JavaScript',
  '.as': 'ActionScript',
  '.mxml': 'ActionScript',
  '.py': 'Python',
  '.go': 'Go',
  '.rs': 'Rust',
  '.java': 'Java',
  '.kt': 'Kotlin',
  '.kts': 'Kotlin',
  '.c': 'C',
  '.h': 'C',
  '.cpp': 'C++',
  '.cc': 'C++',
  '.cxx': 'C++',
  '.hpp': 'C++',
  '.cs': 'C#',
  '.rb': 'Ruby',
  '.php': 'PHP',
  '.swift': 'Swift',
  '.scala': 'Scala',
  '.dart': 'Dart',
  '.ex': 'Elixir',
  '.exs': 'Elixir',
  '.clj': 'Clojure',
  '.hs': 'Haskell',
  '.ml': 'OCaml',
  '.lua': 'Lua',
  '.r': 'R',
  '.jl': 'Julia',
  '.d': 'D',
  '.asm': 'Z80 Assembly',
  '.z80': 'Z80 Assembly',
}

interface FrameworkRule {
  tag: string
  packageNames?: string[]
  files?: string[]
  manifests?: string[]
}

const FRAMEWORK_RULES: FrameworkRule[] = [
  { tag: 'React', packageNames: ['react'] },
  { tag: 'Next.js', packageNames: ['next'], files: ['next.config.js', 'next.config.mjs', 'next.config.ts'] },
  { tag: 'Vue', packageNames: ['vue'] },
  { tag: 'Nuxt', packageNames: ['nuxt'] },
  { tag: 'Angular', packageNames: ['@angular/core', '@angular/cli'] },
  { tag: 'Svelte', packageNames: ['svelte'] },
  { tag: 'SvelteKit', packageNames: ['@sveltejs/kit'] },
  { tag: 'SolidJS', packageNames: ['solid-js'] },
  { tag: 'Astro', packageNames: ['astro'] },
  { tag: 'Electron', packageNames: ['electron', 'electron-vite'] },
  { tag: 'Express', packageNames: ['express'] },
  { tag: 'Fastify', packageNames: ['fastify'] },
  { tag: 'NestJS', packageNames: ['@nestjs/core'] },
  { tag: 'Django', files: ['manage.py'], manifests: ['pyproject.toml', 'requirements.txt'] },
  { tag: 'Flask', manifests: ['requirements.txt', 'pyproject.toml'] },
  { tag: 'FastAPI', manifests: ['requirements.txt', 'pyproject.toml'] },
  { tag: 'Spring', manifests: ['pom.xml', 'build.gradle', 'build.gradle.kts'] },
  { tag: 'Ruby on Rails', files: ['config.ru'], manifests: ['Gemfile'] },
  { tag: 'Unity', files: ['Assets', 'ProjectSettings/ProjectVersion.txt'], manifests: ['Packages/manifest.json'] },
  { tag: 'raylib', files: ['raylib.h'], manifests: ['dub.json', 'dub.sdl', 'CMakeLists.txt'] },
]

function clamp01(value: number): number {
  if (value < 0) return 0
  if (value > 1) return 1
  return value
}

function addScore(scores: Map<string, number>, tag: string, amount: number): void {
  if (!Number.isFinite(amount) || amount <= 0) return
  scores.set(tag, (scores.get(tag) ?? 0) + amount)
}

function listPackageNames(rootDir: string): Set<string> {
  const packageJsonPath = path.join(rootDir, 'package.json')
  try {
    const raw = fs.readFileSync(packageJsonPath, 'utf-8')
    const parsed = JSON.parse(raw) as {
      dependencies?: Record<string, string>
      devDependencies?: Record<string, string>
      peerDependencies?: Record<string, string>
    }
    const names = new Set<string>()
    const pushKeys = (deps?: Record<string, string>) => {
      if (!deps) return
      for (const name of Object.keys(deps)) names.add(name)
    }
    pushKeys(parsed.dependencies)
    pushKeys(parsed.devDependencies)
    pushKeys(parsed.peerDependencies)
    return names
  } catch {
    return new Set<string>()
  }
}

function hasTextInFile(filePath: string, pattern: RegExp): boolean {
  try {
    const content = fs.readFileSync(filePath, 'utf-8')
    return pattern.test(content)
  } catch {
    return false
  }
}

function walkProject(rootDir: string): { extensionCounts: Map<string, number>; fileNames: Set<string>; totalSourceFiles: number } {
  const extensionCounts = new Map<string, number>()
  const fileNames = new Set<string>()
  let totalSourceFiles = 0
  let sampledFiles = 0

  const stack: string[] = [rootDir]
  while (stack.length > 0 && sampledFiles < MAX_SAMPLED_FILES) {
    const current = stack.pop() as string
    let entries: fs.Dirent[]
    try {
      entries = fs.readdirSync(current, { withFileTypes: true })
    } catch {
      continue
    }

    for (const entry of entries) {
      if (sampledFiles >= MAX_SAMPLED_FILES) break
      if (entry.isDirectory()) {
        if (SKIP_DIRS.has(entry.name) || entry.name.startsWith('.')) continue
        stack.push(path.join(current, entry.name))
        continue
      }
      if (!entry.isFile()) continue

      sampledFiles += 1
      fileNames.add(entry.name)
      const ext = path.extname(entry.name).toLowerCase()
      if (!ext) continue
      const language = EXTENSION_LANGUAGES[ext]
      if (!language) continue
      totalSourceFiles += 1
      extensionCounts.set(ext, (extensionCounts.get(ext) ?? 0) + 1)
    }
  }

  return { extensionCounts, fileNames, totalSourceFiles }
}

function applyManifestSignals(rootDir: string, scores: Map<string, number>): void {
  const manifestSignals: Array<{ file: string; tag: string; score: number }> = [
    { file: 'Cargo.toml', tag: 'Rust', score: 0.9 },
    { file: 'go.mod', tag: 'Go', score: 0.9 },
    { file: 'flex-config.xml', tag: 'ActionScript', score: 0.9 },
    { file: 'air-app.xml', tag: 'ActionScript', score: 0.85 },
    { file: 'pyproject.toml', tag: 'Python', score: 0.75 },
    { file: 'requirements.txt', tag: 'Python', score: 0.6 },
    { file: 'setup.py', tag: 'Python', score: 0.65 },
    { file: 'pom.xml', tag: 'Java', score: 0.85 },
    { file: 'build.gradle', tag: 'Java', score: 0.8 },
    { file: 'build.gradle.kts', tag: 'Kotlin', score: 0.8 },
    { file: 'Gemfile', tag: 'Ruby', score: 0.85 },
    { file: 'composer.json', tag: 'PHP', score: 0.8 },
    { file: 'pubspec.yaml', tag: 'Dart', score: 0.85 },
    { file: 'mix.exs', tag: 'Elixir', score: 0.85 },
    { file: 'project.clj', tag: 'Clojure', score: 0.85 },
    { file: 'deps.edn', tag: 'Clojure', score: 0.7 },
    { file: 'dub.json', tag: 'D', score: 0.85 },
    { file: 'dub.sdl', tag: 'D', score: 0.85 },
  ]

  for (const signal of manifestSignals) {
    if (fs.existsSync(path.join(rootDir, signal.file))) {
      addScore(scores, signal.tag, signal.score)
    }
  }
}

function applyFrameworkSignals(rootDir: string, fileNames: Set<string>, packageNames: Set<string>, scores: Map<string, number>): void {
  for (const rule of FRAMEWORK_RULES) {
    let value = 0

    if (rule.packageNames && rule.packageNames.some((name) => packageNames.has(name))) {
      value += 0.9
    }

    if (rule.files && rule.files.some((name) => fileNames.has(name) || fs.existsSync(path.join(rootDir, name)))) {
      value += 0.5
    }

    if (rule.manifests && rule.manifests.some((name) => fs.existsSync(path.join(rootDir, name)))) {
      if (rule.tag === 'Django' && hasTextInFile(path.join(rootDir, 'requirements.txt'), /django/i)) value += 0.8
      else if (rule.tag === 'Flask' && hasTextInFile(path.join(rootDir, 'requirements.txt'), /\bflask\b/i)) value += 0.8
      else if (rule.tag === 'FastAPI' && hasTextInFile(path.join(rootDir, 'requirements.txt'), /fastapi/i)) value += 0.8
      else if (rule.tag === 'Spring' && (
        hasTextInFile(path.join(rootDir, 'pom.xml'), /spring/i)
        || hasTextInFile(path.join(rootDir, 'build.gradle'), /spring/i)
        || hasTextInFile(path.join(rootDir, 'build.gradle.kts'), /spring/i)
      )) {
        value += 0.7
      } else if (rule.tag === 'Ruby on Rails' && hasTextInFile(path.join(rootDir, 'Gemfile'), /rails/i)) {
        value += 0.8
      } else if (rule.tag === 'Unity' && (
        hasTextInFile(path.join(rootDir, 'Packages/manifest.json'), /com\.unity\./i)
        || fs.existsSync(path.join(rootDir, 'ProjectSettings', 'ProjectVersion.txt'))
      )) {
        value += 0.9
      } else if (rule.tag === 'raylib' && (
        hasTextInFile(path.join(rootDir, 'dub.json'), /raylib/i)
        || hasTextInFile(path.join(rootDir, 'dub.sdl'), /raylib/i)
        || hasTextInFile(path.join(rootDir, 'CMakeLists.txt'), /raylib/i)
      )) {
        value += 0.8
      }
    }

    addScore(scores, rule.tag, value)
  }
}

export function detectProjectTags(rootDir: string): ProjectTag[] {
  const scores = new Map<string, number>()
  const { extensionCounts, fileNames, totalSourceFiles } = walkProject(rootDir)

  if (totalSourceFiles > 0) {
    for (const [ext, count] of extensionCounts.entries()) {
      const language = EXTENSION_LANGUAGES[ext]
      if (!language) continue
      addScore(scores, language, count / totalSourceFiles)
    }
  }

  applyManifestSignals(rootDir, scores)
  const packageNames = listPackageNames(rootDir)
  applyFrameworkSignals(rootDir, fileNames, packageNames, scores)

  const totalScore = Array.from(scores.values()).reduce((sum, value) => sum + value, 0)
  if (totalScore <= 0) return []

  return Array.from(scores.entries())
    .map(([name, value]) => ({
      name,
      score: clamp01(value / totalScore),
    }))
    .sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
}
