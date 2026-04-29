import { useAppStore } from '../../store/appStore'
import MarkdownView from './MarkdownView'
import LaunchedProjectsContainer from './LaunchedProjectsContainer'

export default function MainPane() {
  const { selectedProject, launchedProjects } = useAppStore()

  const selectedIsLaunched = selectedProject
    ? launchedProjects.has(selectedProject.path)
    : false

  // Determine which launched project to show (or null = show markdown)
  const activeTerminalPath = selectedIsLaunched ? selectedProject!.path : null

  return (
    <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      {/* Markdown view: shown when no project selected, or selected project not launched */}
      {(!selectedIsLaunched || !selectedProject) && (
        <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
          {selectedProject ? (
            <MarkdownView project={selectedProject} />
          ) : (
            <div style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: 'var(--text-muted)',
              fontSize: 14,
            }}>
              Select a project from the left panel
            </div>
          )}
        </div>
      )}

      {/* Terminal view: mounted only for the active launched project */}
      {activeTerminalPath && (
        <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
          <LaunchedProjectsContainer activeProjectPath={activeTerminalPath} />
        </div>
      )}
    </div>
  )
}
