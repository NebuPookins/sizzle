import { useEffect } from 'react'
import { useAppStore, Project } from './store/appStore'
import LeftPane from './components/LeftPane/LeftPane'
import MainPane from './components/MainPane/MainPane'

export default function App() {
  const { setProjects } = useAppStore()

  useEffect(() => {
    async function load() {
      const [scanned, allMeta] = await Promise.all([
        window.sizzle.scanProjects(),
        window.sizzle.getAllMetadata(),
      ])
      const projects: Project[] = scanned.map((p) => ({
        ...p,
        lastLaunched: allMeta[p.path]?.lastLaunched ?? null,
      }))
      setProjects(projects)
    }
    load()
  }, [setProjects])

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      <LeftPane />
      <MainPane />
    </div>
  )
}
