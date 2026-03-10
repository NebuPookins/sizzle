import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { useAppStore } from '../../store/appStore'
import type { LaunchTarget } from '../../store/appStore'
import { getAgent } from '../../agents'
import XtermPane from './XtermPane'
import FileExplorerPane from './FileExplorerPane'

interface Props {
  projectPath: string
  launchTarget: LaunchTarget
}

const GITHUB_TAB_ID = 'github'

export default function TerminalView({ projectPath, launchTarget }: Props) {
  const {
    setClaudeStatus,
    setShellStatus,
    unlaunchProject,
    terminalStates,
    setActiveTopTab,
    relaunchTerminal,
  } = useAppStore()
  const isShellOnly = launchTarget === 'shell'
  const [agentExited, setAgentExited] = useState(false)
  const [shellExited, setShellExited] = useState(false)
  const [markdownFiles, setMarkdownFiles] = useState<string[]>([])
  const [activeMarkdown, setActiveMarkdown] = useState<string | null>(null)
  const [githubUrl, setGithubUrl] = useState<string | null>(null)
  const terminalState = terminalStates[projectPath]
  const agentSession = terminalState?.agentSession ?? 0
  const shellSession = terminalState?.shellSession ?? 0
  const activeTopTab = terminalState?.activeTopTab ?? 'terminal'
  const shell = window.sizzle.defaultShell || '/bin/bash'
  const agent = isShellOnly ? null : getAgent(launchTarget)
  const [agentArgs, setAgentArgs] = useState<string[] | null>(null)
  const tabName = (filePath: string) => filePath.split('/').pop() ?? filePath

  useEffect(() => {
    if (isShellOnly) {
      if (!shellExited) return
    } else {
      if (!agentExited || !shellExited) return
    }
    const timer = setTimeout(() => unlaunchProject(projectPath), 2000)
    return () => clearTimeout(timer)
  }, [agentExited, shellExited, isShellOnly])

  useEffect(() => {
    if (!agent) return
    let isMounted = true
    setAgentArgs(null)
    agent.getArgs(projectPath).then((args) => {
      if (!isMounted) return
      setAgentArgs(args)
    })
    return () => {
      isMounted = false
    }
  }, [projectPath, launchTarget])

  useEffect(() => {
    let isMounted = true
    setActiveMarkdown(null)
    setGithubUrl(null)

    Promise.all([
      window.sizzle.getMarkdownFiles(projectPath),
      window.sizzle.getProjectRepositoryInfo(projectPath),
    ]).then(([files, repositoryInfo]) => {
      if (!isMounted) return
      setMarkdownFiles(files)
      setGithubUrl(repositoryInfo.githubUrl)
    })

    return () => {
      isMounted = false
    }
  }, [projectPath])

  useEffect(() => {
    if (activeTopTab === 'terminal' || activeTopTab === 'explorer' || activeTopTab === GITHUB_TAB_ID) return

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
      {!isShellOnly && (
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
            {agent!.label}
          </div>
          <div style={{ display: 'flex', alignItems: 'stretch', minWidth: 0, overflowX: 'auto' }}>
            <button
              onClick={() => setActiveTopTab(projectPath, 'terminal')}
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
                onClick={() => setActiveTopTab(projectPath, file)}
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
            {githubUrl && (
              <button
                onClick={() => setActiveTopTab(projectPath, GITHUB_TAB_ID)}
                style={{
                  border: 'none',
                  borderBottom: activeTopTab === GITHUB_TAB_ID ? '2px solid var(--accent)' : '2px solid transparent',
                  background: 'transparent',
                  color: activeTopTab === GITHUB_TAB_ID ? 'var(--text-primary)' : 'var(--text-secondary)',
                  fontSize: 11,
                  fontWeight: activeTopTab === GITHUB_TAB_ID ? 600 : 500,
                  padding: '6px 10px 4px',
                  cursor: 'pointer',
                  whiteSpace: 'nowrap',
                }}
              >
                GitHub
              </button>
            )}
            <button
              onClick={() => setActiveTopTab(projectPath, 'explorer')}
              style={{
                border: 'none',
                borderBottom: activeTopTab === 'explorer' ? '2px solid var(--accent)' : '2px solid transparent',
                background: 'transparent',
                color: activeTopTab === 'explorer' ? 'var(--text-primary)' : 'var(--text-secondary)',
                fontSize: 11,
                fontWeight: activeTopTab === 'explorer' ? 600 : 500,
                padding: '6px 10px 4px',
                cursor: 'pointer',
                whiteSpace: 'nowrap',
              }}
            >
              Explorer
            </button>
          </div>
          <div style={{ flex: 1 }} />
          <button
              onClick={() => {
                window.sizzle.ptyKill(`${launchTarget}-${projectPath}-${agentSession}`)
                window.sizzle.ptyKill(`shell-${projectPath}-${shellSession}`)
                unlaunchProject(projectPath)
              }}
            style={{
              marginRight: 8,
              padding: '3px 9px',
              fontSize: 11,
              fontWeight: 600,
              borderRadius: 4,
              border: '1px solid #5a2020',
              background: '#2a1010',
              color: '#c07070',
              cursor: 'pointer',
              letterSpacing: '0.03em',
            }}
          >
            ■ Stop
          </button>
          {agentExited && (
            <button
              onClick={() => {
                setAgentExited(false)
                relaunchTerminal(projectPath, 'agent')
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
      )}

      {/* Agent terminal (hidden in shell-only mode) */}
      {!isShellOnly && (
        <>
          <div style={{
            flex: 1,
            minHeight: 0,
            display: activeTopTab === 'terminal' ? 'flex' : 'none',
            flexDirection: 'column',
          }}>
            {agentArgs !== null && (
              <XtermPane
                key={`agent-${agentSession}`}
                id={`${launchTarget}-${projectPath}-${agentSession}`}
                cwd={projectPath}
                command={agent!.command}
                args={agentArgs}
                onStatusChange={(status) => setClaudeStatus(projectPath, status)}
                onExit={() => setAgentExited(true)}
              />
            )}
          </div>
          <div style={{
            flex: 1,
            minHeight: 0,
            display: activeTopTab === 'terminal' ? 'none' : 'flex',
            flexDirection: 'column',
            overflowY: activeTopTab === 'explorer' || activeTopTab === GITHUB_TAB_ID ? 'hidden' : 'auto',
            padding: activeTopTab === 'explorer' ? 0 : activeTopTab === GITHUB_TAB_ID ? '14px' : '18px 20px',
            background: '#0f0f1a',
          }}>
            {activeTopTab === 'explorer' && (
              <FileExplorerPane projectPath={projectPath} />
            )}
            {activeTopTab === GITHUB_TAB_ID && githubUrl && (
              <div style={{ display: 'flex', flex: 1, minHeight: 0, flexDirection: 'column', gap: 10 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, color: 'var(--text-secondary)', fontSize: 12 }}>
                  <span>GitHub repository</span>
                  <a href={githubUrl} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)', textDecoration: 'none' }}>
                    Open in browser
                  </a>
                </div>
                <webview
                  src={githubUrl}
                  allowpopups="true"
                  title="GitHub repository"
                  style={{ flex: 1, minHeight: 0, width: '100%', border: '1px solid var(--border)', borderRadius: 8, background: '#fff' }}
                />
              </div>
            )}
            {activeTopTab !== 'explorer' && activeTopTab !== GITHUB_TAB_ID && activeMarkdown === null && (
              <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
            )}
            {activeTopTab !== 'explorer' && activeTopTab !== GITHUB_TAB_ID && activeMarkdown !== null && (
              <div className="markdown-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
                  {activeMarkdown}
                </ReactMarkdown>
              </div>
            )}
          </div>
        </>
      )}

      {/* Divider label / shell header */}
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
        {isShellOnly && (
          <button
            onClick={() => {
              window.sizzle.ptyKill(`shell-${projectPath}-${shellSession}`)
              unlaunchProject(projectPath)
            }}
            style={{
              marginRight: 8,
              padding: '3px 9px',
              fontSize: 11,
              fontWeight: 600,
              borderRadius: 4,
              border: '1px solid #5a2020',
              background: '#2a1010',
              color: '#c07070',
              cursor: 'pointer',
              letterSpacing: '0.03em',
            }}
          >
            ■ Stop
          </button>
        )}
        {shellExited && (
            <button
              onClick={() => {
                setShellExited(false)
                relaunchTerminal(projectPath, 'shell')
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
          onStatusChange={(status) => setShellStatus(projectPath, status)}
          onExit={() => setShellExited(true)}
        />
      </div>
    </div>
  )
}
