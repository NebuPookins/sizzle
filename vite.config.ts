import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import { API_MANIFEST } from './src/shared/manifest-data'

// ── Vite virtual module: re-exports the API manifest from shared data ──

const VIRTUAL_MODULE_ID = 'virtual:api-manifest'
const RESOLVED_VIRTUAL_ID = '\0' + VIRTUAL_MODULE_ID

function apiManifestPlugin(): Plugin {
  const manifestPath = path.resolve(__dirname, 'src/shared/manifest-data.ts')

  return {
    name: 'api-manifest',
    resolveId(id) {
      if (id === VIRTUAL_MODULE_ID) return RESOLVED_VIRTUAL_ID
    },
    load(id) {
      if (id !== RESOLVED_VIRTUAL_ID) return
      this.addWatchFile(manifestPath)
      return `export const API_MANIFEST = ${JSON.stringify(API_MANIFEST)};`
    },
    handleHotUpdate(ctx) {
      if (ctx.file === manifestPath) {
        const mod = ctx.server.moduleGraph.getModuleById(RESOLVED_VIRTUAL_ID)
        if (mod) ctx.server.moduleGraph.invalidateModule(mod)
        ctx.server.ws.send({ type: 'full-reload', path: '*' })
      }
    },
  }
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
