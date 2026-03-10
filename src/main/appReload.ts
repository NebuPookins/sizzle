import fs from 'fs'
import path from 'path'
import crypto from 'crypto'
import { app, BrowserWindow } from 'electron'
import { spawn } from 'child_process'
import type { ReloadSnapshot } from '../shared/reload'
import { RELOAD_STATE_DIR } from './paths'

type QuitMode = 'normal' | 'reload'

let quitMode: QuitMode = 'normal'
let pendingReloadSnapshot: ReloadSnapshot | null = null

function isElectronViteDev(): boolean {
  return process.env.NODE_ENV_ELECTRON_VITE === 'development'
}

function getArgValue(name: string): string | null {
  const prefix = `${name}=`
  const arg = process.argv.find((value) => value.startsWith(prefix))
  return arg ? arg.slice(prefix.length) : null
}

function getReloadStatePath(token: string): string {
  return path.join(RELOAD_STATE_DIR, `${token}.json`)
}

function ensureReloadDir(): void {
  fs.mkdirSync(RELOAD_STATE_DIR, { recursive: true })
}

function loadPendingReloadSnapshot(): void {
  const token = getArgValue('--sizzle-reload-token')
  if (!token) return
  try {
    pendingReloadSnapshot = JSON.parse(fs.readFileSync(getReloadStatePath(token), 'utf8')) as ReloadSnapshot
    fs.rmSync(getReloadStatePath(token), { force: true })
  } catch {
    pendingReloadSnapshot = null
  }
}

loadPendingReloadSnapshot()

export function consumeReloadSnapshot(): ReloadSnapshot | null {
  const snapshot = pendingReloadSnapshot
  pendingReloadSnapshot = null
  return snapshot
}

export async function reloadCore(snapshot: ReloadSnapshot, mainWindow: BrowserWindow): Promise<void> {
  if (isElectronViteDev()) {
    pendingReloadSnapshot = snapshot
    mainWindow.webContents.reloadIgnoringCache()
    return
  }

  ensureReloadDir()
  const token = crypto.randomBytes(16).toString('hex')
  const ackPath = path.join(RELOAD_STATE_DIR, `${token}.ack`)
  const statePath = getReloadStatePath(token)
  fs.writeFileSync(statePath, JSON.stringify(snapshot, null, 2), 'utf8')
  fs.rmSync(ackPath, { force: true })

  const existingArgs = process.argv.slice(1).filter((arg) =>
    !arg.startsWith('--sizzle-reload-token=')
    && !arg.startsWith('--sizzle-reload-ack=')
  )

  const child = spawn(process.execPath, [
    ...existingArgs,
    `--sizzle-reload-token=${token}`,
    `--sizzle-reload-ack=${ackPath}`,
  ], {
    detached: true,
    stdio: 'ignore',
    cwd: process.cwd(),
    env: process.env,
  })
  child.unref()

  const deadline = Date.now() + 15_000
  while (Date.now() < deadline) {
    if (fs.existsSync(ackPath)) {
      quitMode = 'reload'
      fs.rmSync(ackPath, { force: true })
      setTimeout(() => {
        if (!mainWindow.isDestroyed()) {
          mainWindow.close()
        }
        app.quit()
      }, 100)
      return
    }
    await new Promise((resolve) => setTimeout(resolve, 150))
  }

  fs.rmSync(statePath, { force: true })
  throw new Error('Replacement app did not signal readiness')
}

export function signalReloadReady(): void {
  const ackPath = getArgValue('--sizzle-reload-ack')
  if (!ackPath) return
  fs.mkdirSync(path.dirname(ackPath), { recursive: true })
  fs.writeFileSync(ackPath, String(Date.now()), 'utf8')
}

export function getQuitMode(): QuitMode {
  return quitMode
}
