import fs from 'fs'
import net from 'net'
import crypto from 'crypto'
import path from 'path'
import type { PtyRequest, PtyResponse, HostInfo } from '../main/pty/protocol'

// node-pty is native and may be unavailable until rebuilt.
let pty: typeof import('node-pty') | null = null
try {
  pty = require('node-pty')
} catch {
  console.warn('node-pty not available for pty-host')
}

interface PtyEntry {
  process: import('node-pty').IPty | null
  pendingData: string
  history: string
  exitCode: number | null
  cleanupTimer: ReturnType<typeof setTimeout> | null
}

interface ClientState {
  socket: net.Socket
  authenticated: boolean
  buffer: string
}

const HISTORY_LIMIT = 200_000
const EXIT_RETENTION_MS = 60_000
const IDLE_SHUTDOWN_MS = 15_000

const ptys = new Map<string, PtyEntry>()
const clients = new Set<ClientState>()
let idleShutdownTimer: ReturnType<typeof setTimeout> | null = null
let shuttingDown = false

// Use the pre-Electron environment forwarded by the main process, so that
// user shells don't inherit Electron-injected variables like ELECTRON_RUN_AS_NODE.
const shellBaseEnv: NodeJS.ProcessEnv = (() => {
  try {
    if (process.env.SIZZLE_PRE_ELECTRON_ENV) {
      return JSON.parse(process.env.SIZZLE_PRE_ELECTRON_ENV) as NodeJS.ProcessEnv
    }
  } catch {}
  return process.env
})()

function getArgValue(name: string): string | null {
  const prefix = `${name}=`
  const arg = process.argv.find((value) => value.startsWith(prefix))
  return arg ? arg.slice(prefix.length) : null
}

const infoPath = getArgValue('--host-info')
if (!infoPath) {
  throw new Error('Missing --host-info path for pty-host')
}
const hostInfoPath = infoPath

const token = crypto.randomBytes(24).toString('hex')

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
      maybeScheduleIdleShutdown()
    }
  }, EXIT_RETENTION_MS)
}

function broadcast(message: PtyResponse): void {
  const line = JSON.stringify(message) + '\n'
  for (const client of clients) {
    if (!client.authenticated || client.socket.destroyed) continue
    client.socket.write(line)
  }
}

function send(client: ClientState, message: PtyResponse): void {
  if (client.socket.destroyed) return
  client.socket.write(JSON.stringify(message) + '\n')
}

function maybeScheduleIdleShutdown(): void {
  if (shuttingDown) return
  if (clients.size > 0 || ptys.size > 0) {
    if (idleShutdownTimer) {
      clearTimeout(idleShutdownTimer)
      idleShutdownTimer = null
    }
    return
  }
  if (idleShutdownTimer) return
  idleShutdownTimer = setTimeout(() => {
    if (clients.size === 0 && ptys.size === 0) {
      shutdown()
    }
  }, IDLE_SHUTDOWN_MS)
}

function handleCreate(client: ClientState, requestId: string, id: string, cwd: string, command: string, args: string[]): void {
  if (!pty) {
    send(client, { requestId, type: 'error', requestType: 'create', message: 'node-pty unavailable' })
    return
  }

  const existing = ptys.get(id)
  if (existing) {
    clearCleanupTimer(existing)
    send(client, { requestId, type: 'createResult', id, replay: existing.history, exitCode: existing.exitCode })
    return
  }

  const proc = pty.spawn(command, args, {
    name: 'xterm-256color',
    cols: 80,
    rows: 24,
    cwd,
    env: {
      ...shellBaseEnv,
      TERM: 'xterm-256color',
      COLORTERM: 'truecolor',
    },
  })

  const entry: PtyEntry = {
    process: proc,
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
    broadcast({ type: 'event', event: 'pty:exit', id, exitCode })
    scheduleExitedEntryCleanup(id, activeEntry)
  })

  ptys.set(id, entry)
  send(client, { requestId, type: 'createResult', id, replay: '', exitCode: null })
}

function handleRequest(client: ClientState, request: PtyRequest): void {
  try {
    switch (request.type) {
      case 'hello':
        if (request.token !== token) {
          send(client, { requestId: request.requestId, type: 'error', requestType: 'hello', message: 'Unauthorized' })
          client.socket.destroy()
          return
        }
        client.authenticated = true
        send(client, { requestId: request.requestId, type: 'ready' })
        return
      case 'create':
        handleCreate(client, request.requestId, request.id, request.cwd, request.command, request.args)
        return
      case 'write':
        ptys.get(request.id)?.process?.write(request.data)
        send(client, { requestId: request.requestId, type: 'ok', requestType: 'write' })
        return
      case 'resize':
        ptys.get(request.id)?.process?.resize(request.cols, request.rows)
        send(client, { requestId: request.requestId, type: 'ok', requestType: 'resize' })
        return
      case 'detach':
        send(client, { requestId: request.requestId, type: 'ok', requestType: 'detach' })
        return
      case 'kill': {
        const entry = ptys.get(request.id)
        if (entry) {
          clearCleanupTimer(entry)
          try {
            entry.process?.kill()
          } catch {}
          ptys.delete(request.id)
          maybeScheduleIdleShutdown()
        }
        send(client, { requestId: request.requestId, type: 'ok', requestType: 'kill' })
        return
      }
      case 'shutdown':
        send(client, { requestId: request.requestId, type: 'ok', requestType: 'shutdown' })
        shutdown()
        return
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Unknown PTY host error'
    send(client, { requestId: request.requestId, type: 'error', requestType: request.type, message })
  }
}

function shutdown(): void {
  if (shuttingDown) return
  shuttingDown = true
  if (idleShutdownTimer) {
    clearTimeout(idleShutdownTimer)
    idleShutdownTimer = null
  }
  for (const entry of ptys.values()) {
    clearCleanupTimer(entry)
    try {
      entry.process?.kill()
    } catch {}
  }
  ptys.clear()
  try {
    fs.rmSync(hostInfoPath, { force: true })
  } catch {}
  server.close(() => process.exit(0))
  setTimeout(() => process.exit(0), 1000).unref()
}

const flushInterval = setInterval(() => {
  for (const [id, entry] of ptys) {
    if (!entry.pendingData) continue
    const data = entry.pendingData
    entry.pendingData = ''
    broadcast({ type: 'event', event: 'pty:data', id, data })
  }
}, 16)

const server = net.createServer((socket) => {
  if (idleShutdownTimer) {
    clearTimeout(idleShutdownTimer)
    idleShutdownTimer = null
  }

  const client: ClientState = {
    socket,
    authenticated: false,
    buffer: '',
  }
  clients.add(client)

  socket.setEncoding('utf8')

  socket.on('data', (chunk: string) => {
    client.buffer += chunk
    let newlineIndex = client.buffer.indexOf('\n')
    while (newlineIndex >= 0) {
      const line = client.buffer.slice(0, newlineIndex).trim()
      client.buffer = client.buffer.slice(newlineIndex + 1)
      if (line) {
        const request = JSON.parse(line) as PtyRequest
        if (!client.authenticated && request.type !== 'hello') {
          send(client, { requestId: request.requestId, type: 'error', requestType: request.type, message: 'Handshake required' })
          socket.destroy()
          return
        }
        handleRequest(client, request)
      }
      newlineIndex = client.buffer.indexOf('\n')
    }
  })

  socket.on('close', () => {
    clients.delete(client)
    maybeScheduleIdleShutdown()
  })

  socket.on('error', () => {
    clients.delete(client)
    maybeScheduleIdleShutdown()
  })
})

server.on('error', (error) => {
  console.error('PTY host server error', error)
  shutdown()
})

server.listen(0, '127.0.0.1', () => {
  const address = server.address()
  if (!address || typeof address === 'string') {
    throw new Error('Failed to determine PTY host address')
  }
  fs.mkdirSync(path.dirname(infoPath), { recursive: true })
  const info: HostInfo = {
    port: address.port,
    token,
    pid: process.pid,
    startedAt: Date.now(),
  }
  fs.writeFileSync(hostInfoPath, JSON.stringify(info, null, 2), 'utf8')
})

process.on('SIGTERM', shutdown)
process.on('SIGINT', shutdown)
process.on('exit', () => {
  clearInterval(flushInterval)
})
