import { useCallback, useEffect } from 'react'
import { useAppStore, Project } from './store/appStore'
import LeftPane from './components/LeftPane/LeftPane'
import MainPane from './components/MainPane/MainPane'
import type { ProjectTag } from '../preload'

function sortTags(tags: ProjectTag[]): ProjectTag[] {
  return [...tags].sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
}

export default function App() {
  const { setProjects } = useAppStore()

  const loadProjects = useCallback(async () => {
    const [scanned, allMeta] = await Promise.all([
      window.sizzle.scanProjects(),
      window.sizzle.getAllMetadata(),
    ])
    const projects: Project[] = scanned.map((p) => {
      const override = allMeta[p.path]?.tagOverride ?? null
      const tags = sortTags(override?.tags ?? p.detectedTags)
      return {
        ...p,
        lastLaunched: allMeta[p.path]?.lastLaunched ?? null,
        tagOverride: override,
        tags,
        primaryTag: override?.primaryTag ?? tags[0]?.name ?? null,
      }
    })
    setProjects(projects)
  }, [setProjects])

  useEffect(() => {
    loadProjects()
  }, [loadProjects])

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      <LeftPane onRefreshProjects={loadProjects} />
      <MainPane />
    </div>
  )
}
