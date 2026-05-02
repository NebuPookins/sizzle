import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, act, cleanup } from '@testing-library/react'
import TerminalView from '../TerminalView'
import { useAppStore } from '../../../store/appStore'
import type { LaunchTarget } from '../../../store/appStore'

// Mock heavy dependencies (vi.mock is hoisted by vitest before imports)
vi.mock('react-markdown', () => ({ default: () => null }))
vi.mock('remark-gfm', () => ({ default: () => ({}) }))
vi.mock('rehype-highlight', () => ({ default: () => ({}) }))

vi.mock('../../../agents', () => ({
  getAgent: vi.fn(() => ({
    label: 'Mock Agent',
    command: 'mock',
    getArgs: async () => [],
  })),
}))

vi.mock('../XtermPane', () => ({
  default: vi.fn(() => null),
}))

vi.mock('../FileExplorerPane', () => ({
  default: () => null,
}))

vi.mock('../../../api', () => ({
  getDefaultShell: vi.fn(() => Promise.resolve('/bin/bash')),
  getProjectDetail: vi.fn(() => Promise.resolve({ markdownFiles: [], githubUrl: null })),
  readMarkdownFile: vi.fn(() => Promise.resolve('# Content')),
  ptyKill: vi.fn(),
}))

const baseTerminalState = {
  launchTarget: 'shell' as LaunchTarget,
  agentSession: 0,
  shellTabs: [0],
  activeShellTab: 0,
  nextShellSession: 1,
  activeTopTab: 'terminal' as const,
}

function flushMicrotasks() {
  return act(() => Promise.resolve())
}

describe('TerminalView', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    useAppStore.setState({
      projects: [],
      selectedProjectPath: null,
      selectedProject: null,
      launchedProjects: new Set(),
      terminalStates: {},
      claudeStatus: {},
      shellStatus: {},
    })
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('resets stale shell exit state when switching to a different project', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA', '/projectB']),
      terminalStates: {
        '/projectA': { ...baseTerminalState },
        '/projectB': { ...baseTerminalState },
      },
    })

    const mockUnlaunch = vi.fn()
    useAppStore.setState({ unlaunchProject: mockUnlaunch })

    const { rerender } = render(<TerminalView projectPath="/projectA" launchTarget="shell" />)

    // Flush microtasks so getDefaultShell resolves and shell XtermPane renders
    await flushMicrotasks()

    const XtermPane = (await import('../XtermPane')).default as ReturnType<typeof vi.fn>
    const shellPaneProps = XtermPane.mock.lastCall?.[0]
    expect(shellPaneProps).toBeDefined()
    expect(shellPaneProps.id).toBe('shell-/projectA-0')

    // Simulate shell exit for projectA
    await act(() => {
      shellPaneProps.onExit()
    })

    // Switch to projectB before the 2s auto-close timer fires
    rerender(<TerminalView projectPath="/projectB" launchTarget="shell" />)
    await flushMicrotasks()

    // Advance past the auto-close delay
    await act(() => vi.advanceTimersByTimeAsync(5000))

    // unlaunchProject must NOT be called because the stale exitedShells
    // state was reset when projectPath changed
    expect(mockUnlaunch).not.toHaveBeenCalled()
  })

  it('auto-closes project after 2s when all shells exit (shell-only mode)', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': { ...baseTerminalState },
      },
    })

    const mockUnlaunch = vi.fn()
    useAppStore.setState({ unlaunchProject: mockUnlaunch })

    render(<TerminalView projectPath="/projectA" launchTarget="shell" />)
    await flushMicrotasks()

    const XtermPane = (await import('../XtermPane')).default as ReturnType<typeof vi.fn>
    const shellPaneProps = XtermPane.mock.lastCall?.[0]

    // Simulate shell exit
    await act(() => {
      shellPaneProps.onExit()
    })

    // Advance past the 2s auto-close timer
    await act(() => vi.advanceTimersByTimeAsync(3000))

    expect(mockUnlaunch).toHaveBeenCalledTimes(1)
    expect(mockUnlaunch).toHaveBeenCalledWith('/projectA')

    // PTYs should be killed before unlaunching so reopening creates fresh terminals
    const { ptyKill } = await import('../../../api')
    expect(ptyKill).toHaveBeenCalledWith('shell-/projectA-0')
    expect(ptyKill).toHaveBeenCalledTimes(1)
  })

  it('does not auto-close project when shells are still running', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': { ...baseTerminalState },
      },
    })

    const mockUnlaunch = vi.fn()
    useAppStore.setState({ unlaunchProject: mockUnlaunch })

    render(<TerminalView projectPath="/projectA" launchTarget="shell" />)
    await flushMicrotasks()

    // Don't simulate any shell exits — shells are still running

    await act(() => vi.advanceTimersByTimeAsync(3000))

    expect(mockUnlaunch).not.toHaveBeenCalled()
  })

  it('kills all project PTYs before auto-closing when all terminals exit (agent mode)', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': { ...baseTerminalState, launchTarget: 'claude' },
      },
    })

    const mockUnlaunch = vi.fn()
    useAppStore.setState({ unlaunchProject: mockUnlaunch })

    render(<TerminalView projectPath="/projectA" launchTarget="claude" />)
    await flushMicrotasks()

    const XtermPane = (await import('../XtermPane')).default as ReturnType<typeof vi.fn>
    const { ptyKill } = await import('../../../api')

    // Agent renders first (calls[0]), then shell (calls[1])
    const agentProps = XtermPane.mock.calls[0][0]
    const shellProps = XtermPane.mock.calls[1][0]
    expect(agentProps.id).toBe('claude-/projectA-0')
    expect(shellProps.id).toBe('shell-/projectA-0')

    // Shell exits first — auto-close should NOT fire (agent still running)
    await act(() => { shellProps.onExit() })
    await act(() => vi.advanceTimersByTimeAsync(3000))
    expect(mockUnlaunch).not.toHaveBeenCalled()

    // Agent exits — both conditions met, 2s timer starts
    await act(() => { agentProps.onExit() })

    // Advance past the 2s auto-close timer
    await act(() => vi.advanceTimersByTimeAsync(3000))

    expect(mockUnlaunch).toHaveBeenCalledTimes(1)
    expect(mockUnlaunch).toHaveBeenCalledWith('/projectA')

    // Both agent and shell PTYs should be killed
    expect(ptyKill).toHaveBeenCalledWith('claude-/projectA-0')
    expect(ptyKill).toHaveBeenCalledWith('shell-/projectA-0')
    expect(ptyKill).toHaveBeenCalledTimes(2)
  })
})
