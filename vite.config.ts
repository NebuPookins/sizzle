import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
import fs from 'fs'
import type { ApiManifest, ApiManifestEntry } from './src/shared/api-manifest'

// ── Vite virtual module: generates API manifest from api.ts ──

const VIRTUAL_MODULE_ID = 'virtual:api-manifest'
const RESOLVED_VIRTUAL_ID = '\0' + VIRTUAL_MODULE_ID

function apiManifestPlugin(): Plugin {
  return {
    name: 'api-manifest',
    resolveId(id) {
      if (id === VIRTUAL_MODULE_ID) return RESOLVED_VIRTUAL_ID
    },
    load(id) {
      if (id !== RESOLVED_VIRTUAL_ID) return

      const apiPath = path.resolve(__dirname, 'src/renderer/api.ts')
      const content = fs.readFileSync(apiPath, 'utf-8')
      const manifest = parseApiManifest(content)

      return `export const API_MANIFEST = ${JSON.stringify(manifest)};`
    },
  }
}

function parseApiManifest(source: string): ApiManifest {
  const commands: ApiManifestEntry[] = []
  const events: { name: string }[] = []

  // Extract invoke('cmd', { arg1, arg2 }) calls
  // Handles invoke(...) and invoke<Type>(...)
  const invokeRe = /invoke(?:<[^>]+>)?\s*\(\s*['"]([^'"]+)['"]\s*(?:,\s*\{([^}]*)\})?\s*\)/g
  let match: RegExpExecArray | null
  while ((match = invokeRe.exec(source)) !== null) {
    const name = match[1]
    const argsBlock = match[2]
    const args: string[] = []
    if (argsBlock) {
      for (const part of argsBlock.split(',').map((s) => s.trim()).filter(Boolean)) {
        const colonIdx = part.indexOf(':')
        const key = colonIdx >= 0 ? part.slice(0, colonIdx).trim() : part.trim()
        if (key) args.push(key)
      }
    }
    if (!commands.some((c) => c.name === name)) {
      commands.push({ name, args })
    }
  }

  // Extract listen<Type>('event', cb) calls
  const listenRe = /listen(?:<[^>]+>)?\s*\(\s*['"]([^'"]+)['"]/g
  while ((match = listenRe.exec(source)) !== null) {
    const eventName = match[1]
    if (!events.some((e) => e.name === eventName)) {
      events.push({ name: eventName })
    }
  }

  commands.sort((a, b) => a.name.localeCompare(b.name))
  events.sort((a, b) => a.name.localeCompare(b.name))

  return { format: 1, commands, events }
}

// ── Vite config ──

export default defineConfig({
  plugins: [react(), apiManifestPlugin()],
  root: 'src/renderer',
  base: './',
  clearScreen: false,
  server: {
    port: 20742,
    strictPort: false,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari14',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    outDir: path.resolve(__dirname, 'out/renderer'),
    emptyOutDir: true,
  },
})
