import fs from 'fs'
import net from 'net'
import path from 'path'
import { spawn } from 'child_process'
import type { HostInfo, PtyRequest, PtyRequestWithoutId, PtyResponse } from './protocol'
import { PTY_HOST_INFO_PATH } from '../paths'

// Capture the environment as it was before npm and Electron modified process.env.
//
// In dev mode, scripts/dev.mjs runs before Electron and pre-captures the env
// (stripping npm_* vars, which npm exclusively sets) into SIZZLE_PRE_ELECTRON_ENV.
// We prefer that here because it's captured at the earliest possible point.
//
// In production, the Electron binary is exec'd directly from the user's shell with
// no npm wrapper, so /proc/self/environ (Linux) holds the clean pre-Electron env.
// Electron mutates process.env in memory but cannot alter that file.
function capturePreElectronEnv(): NodeJS.ProcessEnv {
  if (process.env.SIZZLE_PRE_ELECTRON_ENV) {
    try {
      return JSON.parse(process.env.SIZZLE_PRE_ELECTRON_ENV) as NodeJS.ProcessEnv
    } catch {
      // fall through
    }
  }
  if (process.platform === 'linux') {
    try {
      const raw = fs.readFileSync('/proc/self/environ')
      const env: NodeJS.ProcessEnv = {}
      for (const entry of raw.toString('latin1').split('\0')) {
        const eq = entry.indexOf('=')
        if (eq === -1) continue
        env[entry.slice(0, eq)] = entry.slice(eq + 1)
      }
      return env
    } catch {
      // fall through
    }
  }
  // Last-resort heuristic: strip known Electron-injected vars.
  const env: NodeJS.ProcessEnv = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (key.startsWith('ELECTRON_')) continue
    env[key] = value
  }
  if (process.env.ORIGINAL_XDG_CURRENT_DESKTOP !== undefined) {
    env['XDG_CURRENT_DESKTOP'] = process.env.ORIGINAL_XDG_CURRENT_DESKTOP
  }
  return env
}

type EventListener = (message: Extract<PtyResponse, { type: 'event' }>) => void

class PtyHostClient {
  private socket: net.Socket | null = null
  private buffer = ''
  private readyPromise: Promise<void> | null = null
  private pendingResolvers = new Map<string, { resolve: (value: unknown) => void; reject: (error: Error) => void }>()
  private eventListener: EventListener | null = null
  private shuttingDown = false
  private requestCounter = 0

  onEvent(listener: EventListener): void {
    this.eventListener = listener
  }

  async ensureReady(): Promise<void> {
    if (this.readyPromise) return this.readyPromise
    this.readyPromise = this.connectOrSpawn()
    return this.readyPromise
  }

  private async connectOrSpawn(): Promise<void> {
    const existing = this.readHostInfo()
    if (existing && await this.tryConnect(existing)) return
    await this.spawnHost()
    const deadline = Date.now() + 10_000
    while (Date.now() < deadline) {
      const info = this.readHostInfo()
      if (info && await this.tryConnect(info)) return
      await new Promise((resolve) => setTimeout(resolve, 150))
    }
    this.readyPromise = null
    throw new Error('Timed out waiting for PTY host to start')
  }

  private readHostInfo(): HostInfo | null {
    try {
      return JSON.parse(fs.readFileSync(PTY_HOST_INFO_PATH, 'utf8')) as HostInfo
    } catch {
      return null
    }
  }

  private async tryConnect(info: HostInfo): Promise<boolean> {
    return new Promise((resolve) => {
      const socket = net.createConnection({ host: '127.0.0.1', port: info.port })
      const requestId = this.nextRequestId()
      let settled = false

      const finish = (ok: boolean) => {
        if (settled) return
        settled = true
        resolve(ok)
      }

      socket.setEncoding('utf8')
      socket.once('error', () => {
        socket.destroy()
        finish(false)
      })

      socket.on('data', (chunk: string) => {
        this.buffer += chunk
        let newlineIndex = this.buffer.indexOf('\n')
        while (newlineIndex >= 0) {
          const line = this.buffer.slice(0, newlineIndex).trim()
          this.buffer = this.buffer.slice(newlineIndex + 1)
          if (line) {
            const response = JSON.parse(line) as PtyResponse
            if (!settled) {
              if (response.type === 'ready' && response.requestId === requestId) {
                this.attachSocket(socket)
                finish(true)
              } else if (response.type === 'error' && response.requestId === requestId) {
                socket.destroy()
                finish(false)
              }
            } else {
              this.handleResponse(response)
            }
          }
          newlineIndex = this.buffer.indexOf('\n')
        }
      })

      socket.once('connect', () => {
        socket.write(JSON.stringify({ requestId, type: 'hello', token: info.token } satisfies PtyRequest) + '\n')
      })
    })
  }

  private attachSocket(socket: net.Socket): void {
    this.socket = socket
    this.buffer = ''
    socket.removeAllListeners('data')
    socket.on('data', (chunk: string) => {
      this.buffer += chunk
      let newlineIndex = this.buffer.indexOf('\n')
      while (newlineIndex >= 0) {
        const line = this.buffer.slice(0, newlineIndex).trim()
        this.buffer = this.buffer.slice(newlineIndex + 1)
        if (line) {
          const response = JSON.parse(line) as PtyResponse
          this.handleResponse(response)
        }
        newlineIndex = this.buffer.indexOf('\n')
      }
    })
    socket.on('close', () => {
      this.socket = null
      this.readyPromise = null
    })
    socket.on('error', () => {
      this.socket = null
      this.readyPromise = null
    })
  }

  private handleResponse(response: PtyResponse): void {
    if (response.type === 'event') {
      this.eventListener?.(response)
      return
    }
    if (response.type === 'ready') return
    if (response.type === 'createResult') {
      const pending = this.pendingResolvers.get(response.requestId)
      if (!pending) return
      this.pendingResolvers.delete(response.requestId)
      pending.resolve({ replay: response.replay, exitCode: response.exitCode })
      return
    }
    if (response.type === 'ok') {
      const pending = this.pendingResolvers.get(response.requestId)
      if (!pending) return
      this.pendingResolvers.delete(response.requestId)
      pending.resolve(undefined)
      return
    }
    if (response.type === 'error') {
      const pending = this.pendingResolvers.get(response.requestId)
      if (!pending) return
      this.pendingResolvers.delete(response.requestId)
      pending.reject(new Error(response.message))
    }
  }

  private nextRequestId(): string {
    this.requestCounter += 1
    return `${process.pid}-${Date.now()}-${this.requestCounter}`
  }

  private async spawnHost(): Promise<void> {
    try {
      fs.rmSync(PTY_HOST_INFO_PATH, { force: true })
    } catch {}
    const scriptPath = path.join(__dirname, 'pty-host.js')
    const child = spawn(process.execPath, [scriptPath, `--host-info=${PTY_HOST_INFO_PATH}`], {
      detached: true,
      stdio: 'ignore',
      env: {
        ...process.env,
        ELECTRON_RUN_AS_NODE: '1',
        // Pass the pre-Electron environment to the daemon so it can spawn
        // user shells with the original environment. The daemon is exec'd
        // from Electron, so its own process.env and /proc/self/environ are
        // already polluted — we must forward the clean env explicitly.
        SIZZLE_PRE_ELECTRON_ENV: JSON.stringify(capturePreElectronEnv()),
      },
    })
    child.unref()
  }

  private async sendRequest<T>(request: PtyRequestWithoutId, mapResult?: (value: unknown) => T): Promise<T> {
    await this.ensureReady()
    if (!this.socket) throw new Error('PTY host is not connected')
    const requestId = this.nextRequestId()
    return new Promise<T>((resolve, reject) => {
      this.pendingResolvers.set(requestId, {
        resolve: (value) => resolve(mapResult ? mapResult(value) : value as T),
        reject,
      })
      this.socket!.write(JSON.stringify({ ...request, requestId }) + '\n')
    })
  }

  create(id: string, cwd: string, command: string, args: string[]): Promise<{ replay: string; exitCode: number | null }> {
    return this.sendRequest({ type: 'create', id, cwd, command, args })
  }

  write(id: string, data: string): Promise<void> {
    return this.sendRequest({ type: 'write', id, data })
  }

  resize(id: string, cols: number, rows: number): Promise<void> {
    return this.sendRequest({ type: 'resize', id, cols, rows })
  }

  detach(id: string): Promise<void> {
    return this.sendRequest({ type: 'detach', id })
  }

  kill(id: string): Promise<void> {
    return this.sendRequest({ type: 'kill', id })
  }

  async shutdown(): Promise<void> {
    if (this.shuttingDown) return
    this.shuttingDown = true
    if (!this.socket && !this.readHostInfo()) return
    try {
      await this.sendRequest({ type: 'shutdown' })
    } catch {}
    this.socket?.destroy()
    this.socket = null
    this.readyPromise = null
  }

  disconnect(): void {
    this.socket?.destroy()
    this.socket = null
    this.readyPromise = null
  }
}

export const ptyHostClient = new PtyHostClient()
