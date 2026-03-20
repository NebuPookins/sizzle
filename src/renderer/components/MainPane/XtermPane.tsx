import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'

interface Props {
  id: string
  cwd: string
  command: string
  args: string[]
  initialCommand?: string
  onStatusChange?: (status: 'working' | 'waiting') => void
  onExit?: () => void
}

export interface XtermPaneHandle {
  focus: () => void
}

const XtermPane = forwardRef<XtermPaneHandle, Props>(function XtermPane({
  id,
  cwd,
  command,
  args,
  initialCommand,
  onStatusChange,
  onExit,
}, ref) {
  const containerRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const onExitRef = useRef(onExit)

  useImperativeHandle(ref, () => ({
    focus: () => termRef.current?.focus(),
  }))

  useEffect(() => {
    onExitRef.current = onExit
  }, [onExit])

  useEffect(() => {
    if (!containerRef.current) return

    const term = new Terminal({
      theme: {
        background: '#0f0f1a',
        foreground: '#e8e8f0',
        cursor: '#7c6af7',
        selectionBackground: '#4040a0',
        black: '#1a1a2e',
        brightBlack: '#4444aa',
        red: '#ff5555',
        brightRed: '#ff6e6e',
        green: '#4caf7d',
        brightGreen: '#6ecf96',
        yellow: '#ffb347',
        brightYellow: '#ffc466',
        blue: '#6272a4',
        brightBlue: '#8be9fd',
        magenta: '#c9b1ff',
        brightMagenta: '#d6bcff',
        cyan: '#8be9fd',
        brightCyan: '#a0f5ff',
        white: '#bfbfbf',
        brightWhite: '#ffffff',
      },
      fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      allowTransparency: true,
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.open(containerRef.current)

    termRef.current = term
    fitRef.current = fitAddon

    // Status tracking for Claude pane
    let statusTimer: ReturnType<typeof setTimeout> | null = null
    let currentStatus: 'working' | 'waiting' = 'waiting'
    const unsubData = window.sizzle.onPtyData((ptyId, data) => {
      if (ptyId !== id) return
      term.write(data)

      if (onStatusChange) {
        if (currentStatus !== 'working') {
          currentStatus = 'working'
          onStatusChange('working')
        }
        if (statusTimer) clearTimeout(statusTimer)
        statusTimer = setTimeout(() => {
          currentStatus = 'waiting'
          onStatusChange('waiting')
        }, 1000)
      }
    })

    const unsubExit = window.sizzle.onPtyExit((ptyId) => {
      if (ptyId !== id) return
      onExitRef.current?.()
    })

    // Send keyboard input to PTY
    const disposeOnData = term.onData((data) => {
      window.sizzle.ptyWrite(id, data)
    })

    // Resize observer — debounced to avoid flooding IPC during window drag
    let resizeTimer: ReturnType<typeof setTimeout> | null = null
    const observer = new ResizeObserver(() => {
      if (resizeTimer) clearTimeout(resizeTimer)
      resizeTimer = setTimeout(() => {
        if (!fitRef.current || !termRef.current) return
        try {
          fitRef.current.fit()
          const { cols, rows } = termRef.current
          window.sizzle.ptyResize(id, cols, rows)
        } catch {}
      }, 100)
    })
    if (containerRef.current) observer.observe(containerRef.current)

    requestAnimationFrame(() => {
      fitAddon.fit()
      window.sizzle.ptyCreate(id, cwd, command, args).then(({ replay, exitCode }) => {
        if (termRef.current !== term) return
        if (replay) term.write(replay)
        if (exitCode !== null) {
          onExitRef.current?.()
          return
        }
        const { cols, rows } = term
        window.sizzle.ptyResize(id, cols, rows)
        if (!initialCommand || replay) return
        // Write after spawn so the shell runs the requested command immediately.
        window.sizzle.ptyWrite(id, `${initialCommand}\r`)
      })
    })

    return () => {
      if (statusTimer) clearTimeout(statusTimer)
      if (resizeTimer) clearTimeout(resizeTimer)
      unsubData()
      unsubExit()
      disposeOnData.dispose()
      observer.disconnect()
      window.sizzle.ptyDetach(id)
      term.dispose()
      termRef.current = null
      fitRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, cwd, command, initialCommand])

  return (
    <div
      ref={containerRef}
      style={{
        flex: 1,
        minHeight: 0,
        overflow: 'hidden',
        background: '#0f0f1a',
      }}
    />
  )
})

export default XtermPane
