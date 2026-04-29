import { useAppStore } from '../../store/appStore'
import TerminalView from './TerminalView'

interface Props {
  activeProjectPath: string | null
}

export default function LaunchedProjectsContainer({ activeProjectPath }: Props) {
  const { terminalStates } = useAppStore()

  if (!activeProjectPath) return null

  return (
    <TerminalView
      projectPath={activeProjectPath}
      launchTarget={terminalStates[activeProjectPath]?.launchTarget ?? 'claude'}
    />
  )
}
