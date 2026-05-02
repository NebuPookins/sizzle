/**
 * Enforce that invoke() / listen() are only called from api-definitions.ts.
 *
 * All IPC with the Rust backend must go through the COMMANDS / EVENTS
 * registry.  This script fails the build if any other file imports from
 * @tauri-apps/api/core (which exports invoke) or @tauri-apps/api/event
 * (which exports listen).
 */

const fs = require('fs')
const path = require('path')

const ROOT = path.resolve(__dirname, '..')
const ALLOWED = [path.resolve(ROOT, 'src/shared/api-definitions.ts')]

const DIRS = ['src/renderer', 'src/shared']

const BAD_IMPORTS = [
  /from ['"]@tauri-apps\/api\/core['"]/,
  /from ['"]@tauri-apps\/api\/event['"]/,
]

let failed = false

for (const dir of DIRS) {
  const absDir = path.resolve(ROOT, dir)
  walk(absDir, (file) => {
    if (ALLOWED.includes(file)) return
    const content = fs.readFileSync(file, 'utf-8')
    for (const re of BAD_IMPORTS) {
      // Skip type-only imports (e.g. `import type { UnlistenFn } from ...`)
      if (re.test(content)) {
        const lines = content.split('\n')
        for (let i = 0; i < lines.length; i++) {
          if (re.test(lines[i]) && !lines[i].includes('import type')) {
            const rel = path.relative(ROOT, file)
            console.error(`\x1b[31mERROR\x1b[0m: ${rel}:${i + 1} imports from @tauri-apps/api directly`)
            console.error(`       All invoke/listen calls must go through COMMANDS/EVENTS in api-definitions.ts`)
            failed = true
          }
        }
      }
    }
  })
}

if (failed) {
  process.exit(1)
}

function walk(dir, fn) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name)
    if (entry.isDirectory() && entry.name !== 'node_modules') walk(abs, fn)
    else if (entry.isFile() && /\.(ts|tsx)$/.test(entry.name)) fn(abs)
  }
}
