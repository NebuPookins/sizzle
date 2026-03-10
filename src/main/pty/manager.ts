import { BrowserWindow } from 'electron'

// node-pty is a native module; import dynamically to handle missing binary gracefully
let pty: typeof import('node-pty') | null = null
try {
  pty = require('node-pty')
} catch {
  console.warn('node-pty not available — PTY features disabled')
}

interface PtyEntry {
  process: import('node-pty').IPty | null
  win: BrowserWindow | null
  pendingData: string
  history: string
  exitCode: number | null
  cleanupTimer: ReturnType<typeof setTimeout> | null
}

const ptys = new Map<string, PtyEntry>()
const HISTORY_LIMIT = 200_000
const EXIT_RETENTION_MS = 60_000

export interface PtyOpenResult {
  replay: string
  exitCode: number | null
}

function appendHistory(entry: PtyEntry, data: string): void {
  entry.history += data
  if (entry.history.length > HISTORY_LIMIT) {
    entry.history = entry.history.slice(-HISTORY_LIMIT)
  }
}

function clearCleanupTimer(entry: PtyEntry): void {
  if (!entry.cleanupTimer) return
  clearTimeout(entry.cleanupTimer)
  entry.cleanupTimer = null
}

function scheduleExitedEntryCleanup(id: string, entry: PtyEntry): void {
  clearCleanupTimer(entry)
  entry.cleanupTimer = setTimeout(() => {
    const current = ptys.get(id)
    if (current === entry && current.process === null) {
      ptys.delete(id)
    }
  }, EXIT_RETENTION_MS)
}

// Flush buffered PTY data once per frame (~16ms) to reduce IPC overhead
const flushInterval = setInterval(() => {
  for (const [id, entry] of ptys) {
    if (entry.pendingData && entry.win && !entry.win.isDestroyed()) {
      entry.win.webContents.send('pty:data', id, entry.pendingData)
      entry.pendingData = ''
    }
  }
}, 16)

export function createPty(
  id: string,
  cwd: string,
  command: string,
  args: string[],
  win: BrowserWindow
): PtyOpenResult {
  if (!pty) throw new Error('node-pty is not available')

  const existing = ptys.get(id)
  if (existing) {
    existing.win = win
    clearCleanupTimer(existing)
    return {
      replay: existing.history,
      exitCode: existing.exitCode,
    }
  }

  const proc = pty.spawn(command, args, {
    name: 'xterm-256color',
    cols: 80,
    rows: 24,
    cwd,
    env: {
      ...process.env,
      TERM: 'xterm-256color',
      COLORTERM: 'truecolor',
    },
  })

  const entry: PtyEntry = {
    process: proc,
    win,
    pendingData: '',
    history: '',
    exitCode: null,
    cleanupTimer: null,
  }

  proc.onData((data: string) => {
    const activeEntry = ptys.get(id)
    if (!activeEntry) return
    activeEntry.pendingData += data
    appendHistory(activeEntry, data)
  })

  proc.onExit(({ exitCode }: { exitCode: number }) => {
    const activeEntry = ptys.get(id)
    if (!activeEntry) return

    activeEntry.process = null
    activeEntry.exitCode = exitCode
    activeEntry.pendingData = ''
    appendHistory(activeEntry, `\r\n\x1b[90m[Process exited: ${exitCode}]\x1b[0m\r\n`)

    if (activeEntry.win && !activeEntry.win.isDestroyed()) {
      activeEntry.win.webContents.send('pty:exit', id, exitCode)
    }
    scheduleExitedEntryCleanup(id, activeEntry)
  })

  ptys.set(id, entry)
  return { replay: '', exitCode: null }
}

export function writePty(id: string, data: string): void {
  ptys.get(id)?.process?.write(data)
}

export function resizePty(id: string, cols: number, rows: number): void {
  ptys.get(id)?.process?.resize(cols, rows)
}

export function detachPty(id: string): void {
  const entry = ptys.get(id)
  if (!entry) return
  entry.win = null
}

export function killPty(id: string): void {
  const entry = ptys.get(id)
  if (entry) {
    clearCleanupTimer(entry)
    try {
      entry.process?.kill()
    } catch {}
    ptys.delete(id)
  }
}

export function killAll(): void {
  clearInterval(flushInterval)
  for (const id of ptys.keys()) {
    killPty(id)
  }
}
