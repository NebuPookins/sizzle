export interface ApiManifestEntry {
  name: string
  args: string[]
}

export interface ApiManifest {
  format: number
  commands: ApiManifestEntry[]
  events: { name: string }[]
}
