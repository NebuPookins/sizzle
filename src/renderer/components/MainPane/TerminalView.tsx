import { useState } from 'react'
import { useAppStore } from '../../store/appStore'
import type { LaunchTarget } from '../../store/appStore'
import XtermPane from './XtermPane'

interface Props {
  projectPath: string
  launchTarget: LaunchTarget
}

export default function TerminalView({ projectPath, launchTarget }: Props) {
  const { setClaudeStatus } = useAppStore()
  const [agentExited, setAgentExited] = useState(false)
  const [agentSession, setAgentSession] = useState(0)
  const [shellExited, setShellExited] = useState(false)
  const [shellSession, setShellSession] = useState(0)
  const shell = window.sizzle.defaultShell || '/bin/bash'
  const agentLabel = launchTarget === 'codex' ? 'Codex' : 'Claude Code'
  const agentCommand = launchTarget === 'codex' ? 'codex' : 'claude'
  const agentArgs = launchTarget === 'codex' ? [] : ['--continue']
  const shellQuote = (value: string) => {
    if (/^[A-Za-z0-9_./-]+$/.test(value)) return value
    return `'${value.replace(/'/g, `'\\''`)}'`
  }
  const agentStartCommand = [agentCommand, ...agentArgs].map(shellQuote).join(' ')

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
        alignItems: 'center',
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
          {agentLabel}
        </div>
        {agentExited && (
          <button
            onClick={() => {
              setAgentExited(false)
              setAgentSession((value) => value + 1)
            }}
            style={{
              marginRight: 10,
              padding: '4px 10px',
              fontSize: 11,
              fontWeight: 600,
              borderRadius: 5,
              border: '1px solid var(--border)',
              background: 'var(--bg-hover)',
              color: 'var(--text-primary)',
              cursor: 'pointer',
            }}
          >
            Relaunch
          </button>
        )}
      </div>

      {/* Agent terminal */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        <XtermPane
          key={`agent-${agentSession}`}
          id={`${launchTarget}-${projectPath}-${agentSession}`}
          cwd={projectPath}
          command={shell}
          args={['-i']}
          initialCommand={agentStartCommand}
          onStatusChange={(status) => setClaudeStatus(projectPath, status)}
          onExit={() => setAgentExited(true)}
        />
      </div>

      {/* Divider label */}
      <div style={{
        display: 'flex',
        flexShrink: 0,
        background: 'var(--bg-panel)',
        alignItems: 'center',
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
          Shell
        </div>
        {shellExited && (
          <button
            onClick={() => {
              setShellExited(false)
              setShellSession((value) => value + 1)
            }}
            style={{
              marginRight: 10,
              padding: '4px 10px',
              fontSize: 11,
              fontWeight: 600,
              borderRadius: 5,
              border: '1px solid var(--border)',
              background: 'var(--bg-hover)',
              color: 'var(--text-primary)',
              cursor: 'pointer',
            }}
          >
            Relaunch
          </button>
        )}
      </div>

      {/* Shell terminal */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        <XtermPane
          key={`shell-${shellSession}`}
          id={`shell-${projectPath}-${shellSession}`}
          cwd={projectPath}
          command={shell}
          args={[]}
          onExit={() => setShellExited(true)}
        />
      </div>
    </div>
  )
}
