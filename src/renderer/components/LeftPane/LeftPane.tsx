import { useState } from 'react'
import { useAppStore } from '../../store/appStore'
import ProjectItem from './ProjectItem'

export default function LeftPane() {
  const { selectedProject, launchedProjects, sortedProjects } = useAppStore()
  const [search, setSearch] = useState('')
  const allProjects = sortedProjects()
  const projects = search
    ? allProjects.filter(p => p.name.toLowerCase().includes(search.toLowerCase()))
    : allProjects

  return (
    <div style={{
      width: 240,
      flexShrink: 0,
      background: 'var(--bg-panel)',
      borderRight: '1px solid var(--border)',
      display: 'flex',
      flexDirection: 'column',
      overflow: 'hidden',
    }}>
      <div style={{
        padding: '14px 12px 10px',
        borderBottom: '1px solid var(--border)',
        fontSize: 13,
        fontWeight: 700,
        letterSpacing: '0.08em',
        textTransform: 'uppercase',
        color: 'var(--text-muted)',
      }}>
        Projects ({projects.length})
      </div>

      <div style={{ padding: '6px 8px', borderBottom: '1px solid var(--border)' }}>
        <input
          type="text"
          placeholder="Filter projects…"
          value={search}
          onChange={e => setSearch(e.target.value)}
          style={{
            width: '100%',
            boxSizing: 'border-box',
            background: 'var(--bg-input, var(--bg-main))',
            border: '1px solid var(--border)',
            borderRadius: 4,
            color: 'var(--text-main)',
            fontSize: 12,
            padding: '4px 8px',
            outline: 'none',
          }}
        />
      </div>

      <div style={{ flex: 1, overflowY: 'auto', padding: '6px 4px' }}>
        {projects.map((project) => (
          <ProjectItem
            key={project.path}
            project={project}
            isSelected={selectedProject?.path === project.path}
            isLaunched={launchedProjects.has(project.path)}
          />
        ))}
        {projects.length === 0 && (
          <div style={{
            padding: '20px 12px',
            color: 'var(--text-muted)',
            fontSize: 12,
            textAlign: 'center',
          }}>
            {search ? 'No matches' : 'Scanning projects…'}
          </div>
        )}
      </div>
    </div>
  )
}
