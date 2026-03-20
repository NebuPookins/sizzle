import { spawn } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const projectRoot = path.resolve(__dirname, '..')
const electronViteCli = path.join(projectRoot, 'node_modules', 'electron-vite', 'bin', 'electron-vite.js')

const rawArgs = process.argv.slice(2)
const electronViteArgs = []
const electronAppArgs = []

let forwardRestToElectron = false

for (const arg of rawArgs) {
  if (forwardRestToElectron) {
    electronAppArgs.push(arg)
    continue
  }

  if (arg === '--') {
    forwardRestToElectron = true
    continue
  }

  if (arg.startsWith('--sizzle-')) {
    electronAppArgs.push(arg)
    continue
  }

  electronViteArgs.push(arg)
}

const childArgs = [electronViteCli, 'dev', ...electronViteArgs]
if (electronAppArgs.length > 0) {
  childArgs.push('--', ...electronAppArgs)
}

const childEnv = { ...process.env }
delete childEnv.ELECTRON_RUN_AS_NODE

// Capture the user's environment before npm and Electron add their own variables.
// npm_ vars are set exclusively by npm (well-defined convention, not a heuristic).
// We do this here because dev.mjs runs before Electron starts.
const preElectronEnv = {}
for (const [key, value] of Object.entries(process.env)) {
  if (key.startsWith('npm_')) continue
  if (value !== undefined) preElectronEnv[key] = value
}
childEnv.SIZZLE_PRE_ELECTRON_ENV = JSON.stringify(preElectronEnv)

const child = spawn(process.execPath, childArgs, {
  cwd: projectRoot,
  stdio: 'inherit',
  env: childEnv,
})

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal)
    return
  }
  process.exit(code ?? 0)
})

child.on('error', (error) => {
  console.error(error)
  process.exit(1)
})
