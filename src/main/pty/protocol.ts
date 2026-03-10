export interface HostInfo {
  port: number
  token: string
  pid: number
  startedAt: number
}

export type PtyRequest =
  | { requestId: string; type: 'hello'; token: string }
  | { requestId: string; type: 'create'; id: string; cwd: string; command: string; args: string[] }
  | { requestId: string; type: 'write'; id: string; data: string }
  | { requestId: string; type: 'resize'; id: string; cols: number; rows: number }
  | { requestId: string; type: 'detach'; id: string }
  | { requestId: string; type: 'kill'; id: string }
  | { requestId: string; type: 'shutdown' }

export type PtyResponse =
  | { requestId: string; type: 'ready' }
  | { requestId: string; type: 'createResult'; id: string; replay: string; exitCode: number | null }
  | { requestId: string; type: 'ok'; requestType: PtyRequest['type'] }
  | { type: 'event'; event: 'pty:data'; id: string; data: string }
  | { type: 'event'; event: 'pty:exit'; id: string; exitCode: number }
  | { requestId: string; type: 'error'; requestType: PtyRequest['type']; message: string }

export type PtyRequestWithoutId =
  PtyRequest extends infer T
    ? T extends { requestId: string }
      ? Omit<T, 'requestId'>
      : never
    : never
