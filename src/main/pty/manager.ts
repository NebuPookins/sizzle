import { BrowserWindow } from 'electron'

// node-pty is a native module; import dynamically to handle missing binary gracefully
let pty: typeof import('node-pty') | null = null
try {
  pty = require('node-pty')
} catch {
  console.warn('node-pty not available — PTY features disabled')
}

interface PtyEntry {
  process: import('node-pty').IPty
  win: BrowserWindow
  pendingData: string
}

const ptys = new Map<string, PtyEntry>()

// Flush buffered PTY data once per frame (~16ms) to reduce IPC overhead
const flushInterval = setInterval(() => {
  for (const [id, entry] of ptys) {
    if (entry.pendingData && !entry.win.isDestroyed()) {
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
): void {
  if (!pty) throw new Error('node-pty is not available')
  if (ptys.has(id)) return // Already running

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

  proc.onData((data: string) => {
    const entry = ptys.get(id)
    if (entry) entry.pendingData += data
  })

  proc.onExit(({ exitCode }: { exitCode: number }) => {
    ptys.delete(id)
    if (!win.isDestroyed()) {
      win.webContents.send('pty:exit', id, exitCode)
    }
  })

  ptys.set(id, { process: proc, win, pendingData: '' })
}

export function writePty(id: string, data: string): void {
  ptys.get(id)?.process.write(data)
}

export function resizePty(id: string, cols: number, rows: number): void {
  ptys.get(id)?.process.resize(cols, rows)
}

export function killPty(id: string): void {
  const entry = ptys.get(id)
  if (entry) {
    try {
      entry.process.kill()
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
