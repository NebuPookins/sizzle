import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
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
  const [markdownFiles, setMarkdownFiles] = useState<string[]>([])
  const [activeTopTab, setActiveTopTab] = useState<'terminal' | string>('terminal')
  const [activeMarkdown, setActiveMarkdown] = useState<string | null>(null)
  const shell = window.sizzle.defaultShell || '/bin/bash'
  const agentLabel = launchTarget === 'codex' ? 'Codex' : 'Claude Code'
  const agentCommand = launchTarget === 'codex' ? 'codex' : 'claude'
  const agentArgs = launchTarget === 'codex' ? [] : ['--continue']
  const shellQuote = (value: string) => {
    if (/^[A-Za-z0-9_./-]+$/.test(value)) return value
    return `'${value.replace(/'/g, `'\\''`)}'`
  }
  const agentStartCommand = [agentCommand, ...agentArgs].map(shellQuote).join(' ')
  const tabName = (filePath: string) => filePath.split('/').pop() ?? filePath

  useEffect(() => {
    let isMounted = true
    setActiveTopTab('terminal')
    setActiveMarkdown(null)

    window.sizzle.getMarkdownFiles(projectPath).then((files) => {
      if (!isMounted) return
      setMarkdownFiles(files)
    })

    return () => {
      isMounted = false
    }
  }, [projectPath])

  useEffect(() => {
    if (activeTopTab === 'terminal') return

    let isMounted = true
    const selectedFile = activeTopTab
    setActiveMarkdown(null)
    window.sizzle.readMarkdownFile(selectedFile).then((content) => {
      if (!isMounted) return
      setActiveMarkdown(content ?? '*Could not read file.*')
    })

    return () => {
      isMounted = false
    }
  }, [activeTopTab])

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
        gap: 8,
      }}>
        <div style={{
          flexShrink: 0,
          padding: '5px 12px',
          fontSize: 11,
          color: 'var(--text-muted)',
          fontWeight: 600,
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
        }}>
          {agentLabel}
        </div>
        {markdownFiles.length > 0 && (
          <div style={{ display: 'flex', alignItems: 'stretch', minWidth: 0, overflowX: 'auto' }}>
            <button
              onClick={() => setActiveTopTab('terminal')}
              style={{
                border: 'none',
                borderBottom: activeTopTab === 'terminal' ? '2px solid var(--accent)' : '2px solid transparent',
                background: 'transparent',
                color: activeTopTab === 'terminal' ? 'var(--text-primary)' : 'var(--text-secondary)',
                fontSize: 11,
                fontWeight: activeTopTab === 'terminal' ? 600 : 500,
                padding: '6px 10px 4px',
                cursor: 'pointer',
                textTransform: 'uppercase',
                letterSpacing: '0.03em',
                whiteSpace: 'nowrap',
              }}
            >
              Terminal
            </button>
            {markdownFiles.map((file) => (
              <button
                key={file}
                onClick={() => setActiveTopTab(file)}
                style={{
                  border: 'none',
                  borderBottom: activeTopTab === file ? '2px solid var(--accent)' : '2px solid transparent',
                  background: 'transparent',
                  color: activeTopTab === file ? 'var(--text-primary)' : 'var(--text-secondary)',
                  fontSize: 11,
                  fontWeight: activeTopTab === file ? 600 : 500,
                  padding: '6px 10px 4px',
                  cursor: 'pointer',
                  whiteSpace: 'nowrap',
                }}
              >
                {tabName(file)}
              </button>
            ))}
          </div>
        )}
        <div style={{ flex: 1 }} />
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
      <div style={{
        flex: 1,
        minHeight: 0,
        display: activeTopTab === 'terminal' ? 'flex' : 'none',
        flexDirection: 'column',
      }}>
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
      <div style={{
        flex: 1,
        minHeight: 0,
        display: activeTopTab === 'terminal' ? 'none' : 'flex',
        flexDirection: 'column',
        overflowY: 'auto',
        padding: '18px 20px',
        background: '#0f0f1a',
      }}>
        {activeMarkdown === null && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
        )}
        {activeMarkdown !== null && (
          <div className="markdown-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
              {activeMarkdown}
            </ReactMarkdown>
          </div>
        )}
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
