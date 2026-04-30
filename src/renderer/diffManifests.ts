import type { ApiManifest, ApiManifestEntry } from '../shared/api-manifest'

export interface ManifestDiffEntry {
  name: string
  kind: 'missing' | 'extra' | 'changed'
  frontendArgs?: string[]
  backendArgs?: string[]
}

export interface ManifestDiff {
  missing: ManifestDiffEntry[]
  extra: ManifestDiffEntry[]
  changed: ManifestDiffEntry[]
}

function buildMap(manifest: ApiManifest): Map<string, string[]> {
  const map = new Map<string, string[]>()
  for (const cmd of manifest.commands) {
    map.set(cmd.name, cmd.args)
  }
  return map
}

function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}

export function diffManifests(frontend: ApiManifest, backend: ApiManifest): ManifestDiff {
  const feMap = buildMap(frontend)
  const beMap = buildMap(backend)

  const missing: ManifestDiffEntry[] = []
  const extra: ManifestDiffEntry[] = []
  const changed: ManifestDiffEntry[] = []

  // Compare commands
  for (const [name, feArgs] of feMap) {
    const beArgs = beMap.get(name)
    if (beArgs === undefined) {
      missing.push({ name, kind: 'missing', frontendArgs: feArgs })
    } else if (!arraysEqual(feArgs, beArgs)) {
      changed.push({ name, kind: 'changed', frontendArgs: feArgs, backendArgs: beArgs })
    }
  }

  for (const [name, beArgs] of beMap) {
    if (!feMap.has(name)) {
      extra.push({ name, kind: 'extra', backendArgs: beArgs })
    }
  }

  return { missing, extra, changed }
}
