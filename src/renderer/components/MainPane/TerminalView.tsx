import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { useAppStore } from '../../store/appStore'
import type { LaunchTarget } from '../../store/appStore'
import { getAgent } from '../../agents'
import XtermPane, { type XtermPaneHandle } from './XtermPane'
import FileExplorerPane from './FileExplorerPane'
import { getDefaultShell, getProjectDetail, readMarkdownFile, ptyKill } from '../../api'

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
    setActiveShellTab,
    createShellTab,
    closeShellTab,
    relaunchAgentTerminal,
    relaunchShellTab,
  } = useAppStore()
  const isShellOnly = launchTarget === 'shell'
  const [agentExited, setAgentExited] = useState(false)
  const [exitedShells, setExitedShells] = useState<number[]>([])
  const [shellActivity, setShellActivity] = useState<Record<number, 'working' | 'waiting'>>({})
  const [focusedPane, setFocusedPane] = useState<'agent' | 'shell' | null>(null)
  const [markdownFiles, setMarkdownFiles] = useState<string[]>([])
  const [activeMarkdown, setActiveMarkdown] = useState<string | null>(null)
  const [githubUrl, setGithubUrl] = useState<string | null>(null)
  const terminalState = terminalStates[projectPath]
  const agentSession = terminalState?.agentSession ?? 0
  const shellTabs = terminalState?.shellTabs ?? [0]
  const activeShellTab = terminalState?.activeShellTab ?? shellTabs[0]
  const activeTopTab = terminalState?.activeTopTab ?? 'terminal'
  const nextShellSession = terminalState?.nextShellSession ?? 1
  const [shell, setShell] = useState<string | null>(null)
  const [agentArgs, setAgentArgs] = useState<string[] | null>(null)
  const agent = isShellOnly
    ? null
    : launchTarget === 'custom' && terminalState?.customAgent
      ? { label: terminalState.customAgent.label, command: terminalState.customAgent.command, getArgs: async () => [] }
      : getAgent(launchTarget)
  const shellRefs = useRef<Map<number, XtermPaneHandle>>(new Map())
  const tabName = (filePath: string) => filePath.split('/').pop() ?? filePath
  const allShellsExited = shellTabs.length > 0 && shellTabs.every((shellSession) => exitedShells.includes(shellSession))
  const activeShellExited = exitedShells.includes(activeShellTab)

  // Reset component-local state when switching to a different project,
  // preventing stale exitedShells from closing the wrong project.
  useEffect(() => {
    setAgentExited(false)
    setExitedShells([])
    setShellActivity({})
  }, [projectPath])

  useEffect(() => {
    if (isShellOnly) {
      if (!allShellsExited) return
    } else {
      if (!agentExited || !allShellsExited) return
    }
    const timer = setTimeout(() => unlaunchProject(projectPath), 2000)
    return () => clearTimeout(timer)
  }, [agentExited, allShellsExited, isShellOnly, projectPath, unlaunchProject])

  useEffect(() => {
    setExitedShells((current) => current.filter((shellSession) => shellTabs.includes(shellSession)))
    setShellActivity((current) => {
      const next = Object.fromEntries(
        Object.entries(current).filter(([shellSession]) => shellTabs.includes(Number(shellSession))),
      ) as Record<number, 'working' | 'waiting'>
      return next
    })
  }, [shellTabs])

  useEffect(() => {
    const overallStatus = Object.values(shellActivity).some((status) => status === 'working') ? 'working' : 'waiting'
    setShellStatus(projectPath, overallStatus)
  }, [projectPath, setShellStatus, shellActivity])

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
    getDefaultShell().then((s) => {
      if (!isMounted) return
      setShell(s)
    })
    return () => { isMounted = false }
  }, [])

  useEffect(() => {
    let isMounted = true
    setActiveMarkdown(null)
    setGithubUrl(null)

    getProjectDetail(projectPath).then((detail) => {
      if (!isMounted) return
      setMarkdownFiles(detail.markdownFiles)
      setGithubUrl(detail.githubUrl)
    })

    return () => {
      isMounted = false
    }
  }, [projectPath])

  // Poll for markdown files being added or removed
  useEffect(() => {
    const id = window.setInterval(() => {
      getProjectDetail(projectPath).then((detail) => {
        setMarkdownFiles((prev) => {
          const same =
            prev.length === detail.markdownFiles.length && prev.every((f, i) => f === detail.markdownFiles[i])
          if (same) return prev
          return detail.markdownFiles
        })
        const currentTab = useAppStore.getState().terminalStates[projectPath]?.activeTopTab ?? 'terminal'
        if (currentTab !== 'terminal' && currentTab !== 'explorer' && currentTab !== GITHUB_TAB_ID) {
          if (!detail.markdownFiles.includes(currentTab)) {
            setActiveTopTab(projectPath, detail.markdownFiles.length > 0 ? detail.markdownFiles[0] : 'terminal')
          }
        }
      })
    }, 5000)
    return () => window.clearInterval(id)
  }, [projectPath, setActiveTopTab])

  useEffect(() => {
    if (activeTopTab === 'terminal' || activeTopTab === 'explorer' || activeTopTab === GITHUB_TAB_ID) return

    let isMounted = true
    const selectedFile = activeTopTab
    setActiveMarkdown(null)
    readMarkdownFile(selectedFile).then((content) => {
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
          background: focusedPane === 'agent' ? 'var(--bg-hover)' : 'var(--bg-panel)',
          alignItems: 'center',
          gap: 8,
          borderLeft: focusedPane === 'agent' ? '3px solid var(--accent)' : '3px solid transparent',
          transition: 'border-left-color 0.1s, background 0.1s',
        }}>
          <div style={{
            flexShrink: 0,
            padding: '5px 12px',
            fontSize: 11,
            color: focusedPane === 'agent' ? 'var(--accent)' : 'var(--text-muted)',
            fontWeight: 600,
            letterSpacing: '0.06em',
            textTransform: 'uppercase',
            transition: 'color 0.1s',
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
                ptyKill(`${launchTarget}-${projectPath}-${agentSession}`)
                for (const shellSession of shellTabs) {
                  ptyKill(`shell-${projectPath}-${shellSession}`)
                }
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
                relaunchAgentTerminal(projectPath)
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
          {activeTopTab === 'terminal' && agentArgs !== null && (
            <div style={{
              flex: 1,
              minHeight: 0,
              display: 'flex',
              flexDirection: 'column',
              opacity: focusedPane === 'shell' ? 0.35 : 1,
              transition: 'opacity 0.15s',
            }}>
              <XtermPane
                key={`agent-${agentSession}`}
                id={`${launchTarget}-${projectPath}-${agentSession}`}
                cwd={projectPath}
                command={agent!.command}
                args={agentArgs}
                onStatusChange={(status) => setClaudeStatus(projectPath, status)}
                onExit={() => setAgentExited(true)}
                onFocus={() => setFocusedPane('agent')}
                onBlur={() => setFocusedPane((p) => p === 'agent' ? null : p)}
              />
            </div>
          )}
          <div style={{
            flex: 1,
            minHeight: 0,
            display: activeTopTab === 'terminal' ? 'none' : 'flex',
            flexDirection: 'column',
            overflowY: activeTopTab === 'explorer' || activeTopTab === GITHUB_TAB_ID ? 'hidden' : 'auto',
            padding: activeTopTab === 'explorer' ? 0 : activeTopTab === GITHUB_TAB_ID ? '14px' : '18px 20px',
            background: '#0f0f1a',
            opacity: focusedPane === 'shell' ? 0.35 : 1,
            transition: 'opacity 0.15s',
          }}>
            {activeTopTab === 'explorer' && (
              <FileExplorerPane projectPath={projectPath} />
            )}
            {activeTopTab === GITHUB_TAB_ID && githubUrl && (
              <div style={{ display: 'flex', flex: 1, minHeight: 0, alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 12 }}>
                <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>GitHub repository</div>
                <a
                  href={githubUrl}
                  target="_blank"
                  rel="noreferrer"
                  style={{
                    padding: '8px 16px',
                    background: 'var(--bg-hover)',
                    border: '1px solid var(--border)',
                    borderRadius: 6,
                    color: 'var(--accent)',
                    fontSize: 13,
                    textDecoration: 'none',
                  }}
                >
                  Open in browser ↗
                </a>
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
          background: focusedPane === 'shell' ? 'var(--bg-hover)' : 'var(--bg-panel)',
          alignItems: 'center',
          gap: 8,
          borderLeft: focusedPane === 'shell' ? '3px solid var(--accent)' : '3px solid transparent',
          transition: 'border-left-color 0.1s, background 0.1s',
        }}>
        <div style={{
          flexShrink: 0,
          padding: '5px 12px',
          fontSize: 11,
          color: focusedPane === 'shell' ? 'var(--accent)' : 'var(--text-muted)',
          fontWeight: 600,
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
          transition: 'color 0.1s',
        }}>
          Shell
        </div>
        <div style={{ display: 'flex', alignItems: 'stretch', minWidth: 0, overflowX: 'auto' }}>
          {shellTabs.map((shellSession, index) => {
            const isActive = shellSession === activeShellTab
            const isExited = exitedShells.includes(shellSession)
            return (
              <div
                key={shellSession}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  borderBottom: isActive ? '2px solid var(--accent)' : '2px solid transparent',
                }}
              >
                <button
                  onClick={() => {
                    setActiveShellTab(projectPath, shellSession)
                    requestAnimationFrame(() => shellRefs.current.get(shellSession)?.focus())
                  }}
                  style={{
                    border: 'none',
                    background: 'transparent',
                    color: isActive ? 'var(--text-primary)' : 'var(--text-secondary)',
                    fontSize: 11,
                    fontWeight: isActive ? 600 : 500,
                    padding: '6px 8px 4px 10px',
                    cursor: 'pointer',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {`Shell ${index + 1}${isExited ? ' • exited' : ''}`}
                </button>
                {shellTabs.length > 1 && (
                  <button
                    onClick={() => {
                      ptyKill(`shell-${projectPath}-${shellSession}`)
                      const focusSession = shellSession === activeShellTab
                        ? shellTabs[Math.max(0, shellTabs.indexOf(shellSession) - 1)] ?? shellTabs[0]
                        : activeShellTab
                      closeShellTab(projectPath, shellSession)
                      requestAnimationFrame(() => shellRefs.current.get(focusSession)?.focus())
                    }}
                    style={{
                      border: 'none',
                      background: 'transparent',
                      color: 'var(--text-muted)',
                      fontSize: 12,
                      padding: '6px 8px 4px 0',
                      cursor: 'pointer',
                    }}
                    aria-label={`Close shell ${index + 1}`}
                    title={`Close shell ${index + 1}`}
                  >
                    ×
                  </button>
                )}
              </div>
            )
          })}
          <button
            onClick={() => {
              const newSession = nextShellSession
              createShellTab(projectPath)
              requestAnimationFrame(() => shellRefs.current.get(newSession)?.focus())
            }}
            style={{
              border: 'none',
              background: 'transparent',
              color: 'var(--text-secondary)',
              fontSize: 14,
              fontWeight: 700,
              padding: '4px 10px',
              cursor: 'pointer',
              whiteSpace: 'nowrap',
            }}
            aria-label="Open another shell tab"
            title="Open another shell tab"
          >
            +
          </button>
        </div>
        <div style={{ flex: 1 }} />
        {isShellOnly && (
          <button
            onClick={() => {
              for (const shellSession of shellTabs) {
                ptyKill(`shell-${projectPath}-${shellSession}`)
              }
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
        {activeShellExited && (
            <button
              onClick={() => {
                setExitedShells((current) => current.filter((shellSession) => shellSession !== activeShellTab))
                setShellActivity((current) => {
                  const next = { ...current }
                  delete next[activeShellTab]
                  return next
                })
                relaunchShellTab(projectPath, activeShellTab)
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
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', opacity: focusedPane === 'agent' ? 0.35 : 1, transition: 'opacity 0.15s' }}>
        {shellTabs.map((shellSession) => (
          shellSession === activeShellTab && shell !== null && (
            <div
              key={`shell-pane-${shellSession}`}
              style={{
                flex: 1,
                minHeight: 0,
                display: 'flex',
                flexDirection: 'column',
              }}
            >
              <XtermPane
                key={`shell-${shellSession}`}
                ref={(handle) => {
                  if (handle) shellRefs.current.set(shellSession, handle)
                  else shellRefs.current.delete(shellSession)
                }}
                id={`shell-${projectPath}-${shellSession}`}
                cwd={projectPath}
                command={shell}
                args={[]}
                initialCommand={terminalState?.initialCommand}
                onStatusChange={(status) => {
                  setShellActivity((current) => ({ ...current, [shellSession]: status }))
                }}
                onExit={() => {
                  setExitedShells((current) => (current.includes(shellSession) ? current : [...current, shellSession]))
                  setShellActivity((current) => ({ ...current, [shellSession]: 'waiting' }))
                }}
                onFocus={() => setFocusedPane('shell')}
                onBlur={() => setFocusedPane((p) => p === 'shell' ? null : p)}
              />
            </div>
          )
        ))}
      </div>
    </div>
  )
}
