import { useState, useEffect } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { LaunchTarget, Project, useAppStore } from '../../store/appStore'
import FileExplorerPane from './FileExplorerPane'
import type { ProjectTag, ProjectTagOverride } from '../../../preload'

interface Props {
  project: Project
}

export default function MarkdownView({ project }: Props) {
  const [files, setFiles] = useState<string[]>([])
  const [activeFile, setActiveFile] = useState<'explorer' | string | null>(null)
  const [content, setContent] = useState<string | null>(null)
  const [isEditingTags, setIsEditingTags] = useState(false)
  const [tagInput, setTagInput] = useState('')
  const [primaryTagInput, setPrimaryTagInput] = useState('')
  const { launchProject, setProjectTagOverride } = useAppStore()

  useEffect(() => {
    setFiles([])
    setActiveFile(null)
    setContent(null)

    window.sizzle.getMarkdownFiles(project.path).then((f) => {
      setFiles(f)
      if (f.length > 0) setActiveFile(f[0])
      else setActiveFile('explorer')
    })
  }, [project.path])

  useEffect(() => {
    if (!activeFile || activeFile === 'explorer') return
    setContent(null)
    window.sizzle.readMarkdownFile(activeFile).then((c) => {
      setContent(c ?? '*Could not read file.*')
    })
  }, [activeFile])

  useEffect(() => {
    setIsEditingTags(false)
    setTagInput(project.tags.map((tag) => tag.name).join(', '))
    setPrimaryTagInput(project.primaryTag ?? '')
  }, [project.path, project.tags, project.primaryTag])

  async function handleLaunch(target: LaunchTarget) {
    await window.sizzle.setLastLaunched(project.path)
    launchProject(project, target)
  }

  const tabName = (f: string) => f.split('/').pop() ?? f

  function formatTagPercent(score: number): string {
    return `${Math.round(score * 100)}%`
  }

  function parseTagList(input: string): string[] {
    const unique = new Set<string>()
    for (const part of input.split(',')) {
      const tag = part.trim().replace(/\s+/g, ' ')
      if (tag) unique.add(tag)
    }
    return Array.from(unique)
  }

  async function saveTagOverride() {
    const tagNames = parseTagList(tagInput)
    if (tagNames.length === 0) {
      const updated = await window.sizzle.setTagOverride(project.path, null)
      setProjectTagOverride(project.path, updated.tagOverride)
      setIsEditingTags(false)
      return
    }

    const scoreMap = new Map<string, number>()
    for (const tag of project.tags) {
      scoreMap.set(tag.name, tag.score)
    }
    const fallbackBase = tagNames.length + 1
    const tags: ProjectTag[] = tagNames.map((name, index) => ({
      name,
      score: scoreMap.get(name) ?? 1 / (fallbackBase + index),
    }))

    const chosenPrimary = primaryTagInput.trim()
    const override: ProjectTagOverride = {
      tags,
      primaryTag: tagNames.includes(chosenPrimary) ? chosenPrimary : (tagNames[0] ?? null),
    }
    const updated = await window.sizzle.setTagOverride(project.path, override)
    setProjectTagOverride(project.path, updated.tagOverride)
    setIsEditingTags(false)
  }

  async function clearTagOverride() {
    const updated = await window.sizzle.setTagOverride(project.path, null)
    setProjectTagOverride(project.path, updated.tagOverride)
    setTagInput(project.detectedTags.map((tag) => tag.name).join(', '))
    setPrimaryTagInput(project.detectedTags[0]?.name ?? '')
    setIsEditingTags(false)
  }

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

      <div style={{
        padding: '10px 16px 12px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)' }}>Overview</div>
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
            {!isEditingTags && (
              <button
                onClick={() => setIsEditingTags(true)}
                style={{
                  background: 'var(--bg-hover)',
                  color: 'var(--text-primary)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  padding: '4px 8px',
                  fontSize: 11,
                  cursor: 'pointer',
                }}
              >
                Edit tags
              </button>
            )}
            {project.tagOverride && !isEditingTags && (
              <button
                onClick={clearTagOverride}
                style={{
                  background: 'transparent',
                  color: 'var(--text-muted)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  padding: '4px 8px',
                  fontSize: 11,
                  cursor: 'pointer',
                }}
              >
                Reset auto
              </button>
            )}
          </div>
        </div>

        {!isEditingTags && (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            {project.tags.length === 0 && (
              <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>No tags detected.</span>
            )}
            {project.tags.map((tag) => {
              const isPrimary = project.primaryTag === tag.name
              return (
                <div
                  key={tag.name}
                  style={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '4px 8px',
                    borderRadius: 999,
                    border: `1px solid ${isPrimary ? 'var(--accent)' : 'var(--border)'}`,
                    background: isPrimary ? 'var(--bg-selected)' : 'var(--bg-hover)',
                    fontSize: 11,
                    color: isPrimary ? 'var(--text-primary)' : 'var(--text-secondary)',
                  }}
                >
                  <span>{tag.name}</span>
                  <span style={{ color: 'var(--text-muted)' }}>{formatTagPercent(tag.score)}</span>
                </div>
              )
            })}
          </div>
        )}

        {isEditingTags && (
          <div style={{ display: 'grid', gap: 6, gridTemplateColumns: '1fr auto auto' }}>
            <input
              type="text"
              value={tagInput}
              onChange={(event) => setTagInput(event.target.value)}
              placeholder="Tags (comma-separated)"
              style={{
                background: 'var(--bg-input, var(--bg-main))',
                border: '1px solid var(--border)',
                borderRadius: 4,
                color: 'var(--text-main)',
                fontSize: 12,
                padding: '6px 8px',
                outline: 'none',
              }}
            />
            <input
              type="text"
              value={primaryTagInput}
              onChange={(event) => setPrimaryTagInput(event.target.value)}
              placeholder="Primary"
              style={{
                width: 140,
                background: 'var(--bg-input, var(--bg-main))',
                border: '1px solid var(--border)',
                borderRadius: 4,
                color: 'var(--text-main)',
                fontSize: 12,
                padding: '6px 8px',
                outline: 'none',
              }}
            />
            <div style={{ display: 'flex', gap: 6 }}>
              <button
                onClick={() => setIsEditingTags(false)}
                style={{
                  background: 'transparent',
                  color: 'var(--text-muted)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  padding: '6px 10px',
                  fontSize: 11,
                  cursor: 'pointer',
                }}
              >
                Cancel
              </button>
              <button
                onClick={saveTagOverride}
                style={{
                  background: 'var(--accent)',
                  color: '#fff',
                  border: 'none',
                  borderRadius: 4,
                  padding: '6px 10px',
                  fontSize: 11,
                  cursor: 'pointer',
                }}
              >
                Save
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Tabs */}
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
        <button
          onClick={() => setActiveFile('explorer')}
          style={{
            background: 'none',
            border: 'none',
            borderBottom: activeFile === 'explorer' ? '2px solid var(--accent)' : '2px solid transparent',
            color: activeFile === 'explorer' ? 'var(--text-primary)' : 'var(--text-secondary)',
            padding: '8px 14px',
            fontSize: 12,
            cursor: 'pointer',
            fontWeight: activeFile === 'explorer' ? 600 : 400,
            whiteSpace: 'nowrap',
          }}
        >
          Explorer
        </button>
      </div>

      {/* Content */}
      <div style={{ flex: 1, minHeight: 0, overflowY: activeFile === 'explorer' ? 'hidden' : 'auto', padding: activeFile === 'explorer' ? 0 : '20px 24px' }}>
        {activeFile === 'explorer' && (
          <FileExplorerPane projectPath={project.path} />
        )}
        {activeFile !== 'explorer' && content === null && activeFile && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
        )}
        {activeFile !== 'explorer' && content !== null && (
          <div className="markdown-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
              {content}
            </ReactMarkdown>
          </div>
        )}
        {files.length === 0 && content === null && activeFile !== 'explorer' && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>
            No markdown files found in this project.
          </div>
        )}
      </div>
    </div>
  )
}
