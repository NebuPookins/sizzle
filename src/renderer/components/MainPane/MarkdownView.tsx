import { useState, useEffect } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { LaunchTarget, Project, useAppStore } from '../../store/appStore'

interface Props {
  project: Project
}

export default function MarkdownView({ project }: Props) {
  const [files, setFiles] = useState<string[]>([])
  const [activeFile, setActiveFile] = useState<string | null>(null)
  const [content, setContent] = useState<string | null>(null)
  const { launchProject } = useAppStore()

  useEffect(() => {
    setFiles([])
    setActiveFile(null)
    setContent(null)

    window.sizzle.getMarkdownFiles(project.path).then((f) => {
      setFiles(f)
      if (f.length > 0) setActiveFile(f[0])
    })
  }, [project.path])

  useEffect(() => {
    if (!activeFile) return
    setContent(null)
    window.sizzle.readMarkdownFile(activeFile).then((c) => {
      setContent(c ?? '*Could not read file.*')
    })
  }, [activeFile])

  async function handleLaunch(target: LaunchTarget) {
    await window.sizzle.setLastLaunched(project.path)
    launchProject(project, target)
  }

  const tabName = (f: string) => f.split('/').pop() ?? f

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '10px 16px',
        borderBottom: '1px solid var(--border)',
        flexShrink: 0,
      }}>
        <div style={{ fontWeight: 700, fontSize: 16, flex: 1 }}>{project.name}</div>
        <div style={{ fontSize: 11, color: 'var(--text-muted)', flex: 1 }}>{project.path}</div>
        <button
          onClick={() => handleLaunch('claude')}
          style={{
            background: 'var(--accent)',
            color: '#fff',
            border: 'none',
            borderRadius: 6,
            padding: '7px 18px',
            fontSize: 13,
            fontWeight: 600,
            cursor: 'pointer',
            flexShrink: 0,
          }}
          onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.background = 'var(--accent-hover)' }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.background = 'var(--accent)' }}
        >
          Launch Claude
        </button>
        <button
          onClick={() => handleLaunch('codex')}
          style={{
            background: 'var(--bg-hover)',
            color: 'var(--text-primary)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            padding: '7px 14px',
            fontSize: 13,
            fontWeight: 600,
            cursor: 'pointer',
            flexShrink: 0,
          }}
          onMouseEnter={(e) => { (e.currentTarget as HTMLButtonElement).style.background = 'var(--bg-selected)' }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLButtonElement).style.background = 'var(--bg-hover)' }}
        >
          Launch Codex
        </button>
      </div>

      {/* Tabs */}
      {files.length > 0 && (
        <div style={{
          display: 'flex',
          gap: 0,
          padding: '0 16px',
          borderBottom: '1px solid var(--border)',
          flexShrink: 0,
          overflowX: 'auto',
        }}>
          {files.map((f) => (
            <button
              key={f}
              onClick={() => setActiveFile(f)}
              style={{
                background: 'none',
                border: 'none',
                borderBottom: f === activeFile ? '2px solid var(--accent)' : '2px solid transparent',
                color: f === activeFile ? 'var(--text-primary)' : 'var(--text-secondary)',
                padding: '8px 14px',
                fontSize: 12,
                cursor: 'pointer',
                fontWeight: f === activeFile ? 600 : 400,
                whiteSpace: 'nowrap',
              }}
            >
              {tabName(f)}
            </button>
          ))}
        </div>
      )}

      {/* Content */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
        {content === null && activeFile && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
        )}
        {content !== null && (
          <div className="markdown-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
              {content}
            </ReactMarkdown>
          </div>
        )}
        {files.length === 0 && content === null && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>
            No markdown files found in this project.
          </div>
        )}
      </div>
    </div>
  )
}
