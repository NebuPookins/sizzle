import { useAppStore } from '../../store/appStore'
import XtermPane from './XtermPane'

interface Props {
  projectPath: string
}

export default function TerminalView({ projectPath }: Props) {
  const { setClaudeStatus } = useAppStore()
  const shell = '/bin/bash'

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100%',
      gap: 1,
      background: 'var(--border)',
    }}>
      {/* Label bar */}
      <div style={{
        display: 'flex',
        flexShrink: 0,
        background: 'var(--bg-panel)',
      }}>
        <div style={{
          flex: 1,
          padding: '5px 12px',
          fontSize: 11,
          color: 'var(--text-muted)',
          fontWeight: 600,
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
        }}>
          Claude Code
        </div>
      </div>

      {/* Claude terminal */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        <XtermPane
          id={`claude-${projectPath}`}
          cwd={projectPath}
          command="claude"
          args={['--continue']}
          onStatusChange={(status) => setClaudeStatus(projectPath, status)}
        />
      </div>

      {/* Divider label */}
      <div style={{
        flexShrink: 0,
        background: 'var(--bg-panel)',
        padding: '5px 12px',
        fontSize: 11,
        color: 'var(--text-muted)',
        fontWeight: 600,
        letterSpacing: '0.06em',
        textTransform: 'uppercase',
      }}>
        Shell
      </div>

      {/* Shell terminal */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        <XtermPane
          id={`shell-${projectPath}`}
          cwd={projectPath}
          command={shell}
          args={[]}
        />
      </div>
    </div>
  )
}
