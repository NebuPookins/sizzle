import os from 'os'
import path from 'path'

function getArgValue(name: string): string | null {
  const prefix = `${name}=`
  const arg = process.argv.find((value) => value.startsWith(prefix))
  return arg ? arg.slice(prefix.length) : null
}

const configDirArg = getArgValue('--sizzle-config-dir')

export const SIZZLE_CONFIG_DIR = configDirArg
  ? path.resolve(configDirArg)
  : path.join(os.homedir(), '.config', 'sizzle')
export const DB_PATH = path.join(SIZZLE_CONFIG_DIR, 'db.json')
export const PTY_HOST_INFO_PATH = path.join(SIZZLE_CONFIG_DIR, 'pty-host.json')
export const RELOAD_STATE_DIR = path.join(SIZZLE_CONFIG_DIR, 'reload')
