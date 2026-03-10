import { useCallback, useEffect, useRef } from 'react'
import { useAppStore, Project } from './store/appStore'
import LeftPane from './components/LeftPane/LeftPane'
import MainPane from './components/MainPane/MainPane'
import type { ProjectTag } from '../preload'

const PROJECT_REFRESH_INTERVAL_MS = 10_000

function sortTags(tags: ProjectTag[]): ProjectTag[] {
  return [...tags].sort((a, b) => b.score - a.score || a.name.localeCompare(b.name))
}

export default function App() {
  const { setProjects, hydrateReloadSnapshot, reloadMessage, setReloadMessage } = useAppStore()
  const isLoadingProjectsRef = useRef(false)

  const loadProjects = useCallback(async () => {
    if (isLoadingProjectsRef.current) return
    isLoadingProjectsRef.current = true
    const [scanned, allMeta] = await Promise.all([
      window.sizzle.scanProjects(),
      window.sizzle.getAllMetadata(),
    ])
      .finally(() => {
        isLoadingProjectsRef.current = false
      })

    const projects: Project[] = scanned.map((p) => {
      const override = allMeta[p.path]?.tagOverride ?? null
      const tags = sortTags(override?.tags ?? p.detectedTags)
      return {
        ...p,
        lastLaunched: allMeta[p.path]?.lastLaunched ?? null,
        tagOverride: override,
        marker: allMeta[p.path]?.marker ?? null,
        tags,
        primaryTag: override?.primaryTag ?? tags[0]?.name ?? null,
      }
    })
    setProjects(projects)
  }, [setProjects])

  useEffect(() => {
    window.sizzle.consumeReloadSnapshot().then((snapshot) => {
      if (snapshot) {
        hydrateReloadSnapshot(snapshot)
      }
      void loadProjects()
    })
  }, [hydrateReloadSnapshot, loadProjects])

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void loadProjects()
    }, PROJECT_REFRESH_INTERVAL_MS)

    const refreshOnFocus = () => {
      void loadProjects()
    }
    const refreshOnVisibility = () => {
      if (document.visibilityState === 'visible') {
        void loadProjects()
      }
    }

    window.addEventListener('focus', refreshOnFocus)
    document.addEventListener('visibilitychange', refreshOnVisibility)

    return () => {
      window.clearInterval(intervalId)
      window.removeEventListener('focus', refreshOnFocus)
      document.removeEventListener('visibilitychange', refreshOnVisibility)
    }
  }, [loadProjects])

  useEffect(() => {
    if (!reloadMessage) return
    const timer = window.setTimeout(() => setReloadMessage(null), 4000)
    return () => window.clearTimeout(timer)
  }, [reloadMessage, setReloadMessage])

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      {reloadMessage && (
        <div style={{
          position: 'fixed',
          top: 12,
          left: '50%',
          transform: 'translateX(-50%)',
          zIndex: 3000,
          background: 'var(--bg-panel)',
          border: '1px solid var(--border)',
          borderRadius: 8,
          padding: '10px 14px',
          fontSize: 12,
          color: 'var(--text-primary)',
          boxShadow: '0 8px 24px rgba(0, 0, 0, 0.35)',
        }}>
          {reloadMessage}
        </div>
      )}
      <LeftPane onRefreshProjects={loadProjects} />
      <MainPane />
    </div>
  )
}
