import os from 'os'
import path from 'path'

export const SIZZLE_CONFIG_DIR = path.join(os.homedir(), '.config', 'sizzle')
export const PTY_HOST_INFO_PATH = path.join(SIZZLE_CONFIG_DIR, 'pty-host.json')
export const RELOAD_STATE_DIR = path.join(SIZZLE_CONFIG_DIR, 'reload')
