import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, act, cleanup, screen, fireEvent } from '@testing-library/react'
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

  it('closes the previous project immediately when switching projects while auto-close is pending', async () => {
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

    // Simulate shell exit for projectA — this starts the 2s auto-close timer
    await act(() => {
      shellPaneProps.onExit()
    })

    // Switch to projectB before the 2s timer fires
    rerender(<TerminalView projectPath="/projectB" launchTarget="shell" />)
    await flushMicrotasks()

    // The old project should close immediately instead of waiting for the timer
    expect(mockUnlaunch).toHaveBeenCalledTimes(1)
    expect(mockUnlaunch).toHaveBeenCalledWith('/projectA')

    // PTYs should be killed before unlaunching
    const { ptyKill } = await import('../../../api')
    expect(ptyKill).toHaveBeenCalledWith('shell-/projectA-0')
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

  it('sets focusedPane to agent when clicking non-terminal content after shell was focused', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': {
          ...baseTerminalState,
          launchTarget: 'claude',
          focusedPane: 'shell',
        },
      },
    })

    const { getProjectDetail, readMarkdownFile } = await import('../../../api')
    vi.mocked(getProjectDetail).mockResolvedValue({
      markdownFiles: ['/projectA/README.md'],
      githubUrl: null,
    })
    vi.mocked(readMarkdownFile).mockResolvedValue('# Content')

    render(<TerminalView projectPath="/projectA" launchTarget="claude" />)
    await flushMicrotasks()

    // Switch to the markdown tab
    act(() => {
      useAppStore.getState().setActiveTopTab('/projectA', '/projectA/README.md')
    })
    await flushMicrotasks()

    // Click an element inside the non-terminal content area — event bubbles to wrapper
    fireEvent.mouseDown(screen.getByText('Edit'))

    expect(useAppStore.getState().terminalStates['/projectA'].focusedPane).toBe('agent')
  })

  it('switches to the terminal tab and calls requestAnimationFrame when clicking the Terminal tab', async () => {
    const rafSpy = vi.spyOn(window, 'requestAnimationFrame')

    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': {
          ...baseTerminalState,
          launchTarget: 'claude',
          activeTopTab: '/projectA/README.md',
        },
      },
    })

    const { getProjectDetail } = await import('../../../api')
    vi.mocked(getProjectDetail).mockResolvedValue({
      markdownFiles: ['/projectA/README.md'],
      githubUrl: null,
    })

    render(<TerminalView projectPath="/projectA" launchTarget="claude" />)
    await flushMicrotasks()

    // Click the "Terminal" tab button
    fireEvent.click(screen.getByText('Terminal'))

    expect(useAppStore.getState().terminalStates['/projectA'].activeTopTab).toBe('terminal')
    expect(rafSpy).toHaveBeenCalledTimes(1)

    rafSpy.mockRestore()
  })

  it('focuses the markdown editor when switching to a markdown tab', async () => {
    useAppStore.setState({
      launchedProjects: new Set(['/projectA']),
      terminalStates: {
        '/projectA': { ...baseTerminalState, launchTarget: 'claude' },
      },
    })

    const { getProjectDetail } = await import('../../../api')
    vi.mocked(getProjectDetail).mockResolvedValue({
      markdownFiles: ['/projectA/README.md'],
      githubUrl: null,
    })

    render(<TerminalView projectPath="/projectA" launchTarget="claude" />)
    await flushMicrotasks()

    // Switch to the markdown tab
    act(() => {
      useAppStore.getState().setActiveTopTab('/projectA', '/projectA/README.md')
    })
    await flushMicrotasks()

    // The markdown editor's view container (tabIndex=-1) should have received focus
    const editButton = screen.getByText('Edit')
    const container = editButton.closest('[tabindex="-1"]')
    expect(document.activeElement).toBe(container)
  })
})
