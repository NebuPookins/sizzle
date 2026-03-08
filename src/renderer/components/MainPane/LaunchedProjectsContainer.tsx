import { useAppStore } from '../../store/appStore'
import TerminalView from './TerminalView'

interface Props {
  activeProjectPath: string | null
}

export default function LaunchedProjectsContainer({ activeProjectPath }: Props) {
  const { launchedProjects, launchTargets } = useAppStore()
  const paths = Array.from(launchedProjects)

  return (
    <>
      {paths.map((projectPath) => (
        <div
          key={projectPath}
          style={{
            display: activeProjectPath === projectPath ? 'flex' : 'none',
            flex: 1,
            minHeight: 0,
            flexDirection: 'column',
          }}
        >
          <TerminalView
            projectPath={projectPath}
            launchTarget={launchTargets[projectPath] ?? 'claude'}
          />
        </div>
      ))}
    </>
  )
}
