import { useState, useEffect } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { LaunchTarget, Project, useAppStore } from '../../store/appStore'
import FileExplorerPane from './FileExplorerPane'
import {
  getMarkdownFiles,
  getProjectRepositoryInfo,
  readMarkdownFile,
  setLastLaunched,
  setTagOverride,
  rescanProjectTags,
  getAgentPresets,
} from '../../api'
import type { ProjectTag, ProjectTagOverride, AgentPreset } from '../../api'

interface Props {
  project: Project
}

const GITHUB_TAB_ID = 'github'

export default function MarkdownView({ project }: Props) {
  const [files, setFiles] = useState<string[]>([])
  const [activeFile, setActiveFile] = useState<'explorer' | typeof GITHUB_TAB_ID | string | null>(null)
  const [content, setContent] = useState<string | null>(null)
  const [githubUrl, setGithubUrl] = useState<string | null>(null)
  const [isEditingTags, setIsEditingTags] = useState(false)
  const [tagInput, setTagInput] = useState('')
  const [primaryTagInput, setPrimaryTagInput] = useState('')
  const [customPresets, setCustomPresets] = useState<AgentPreset[]>([])
  const { launchProject, setProjectTagOverride, setProjectDetectedTags, setCustomAgentInfo } = useAppStore()

  useEffect(() => {
    setFiles([])
    setActiveFile(null)
    setContent(null)
    setGithubUrl(null)

    Promise.all([
      getMarkdownFiles(project.path),
      getProjectRepositoryInfo(project.path),
      getAgentPresets(),
    ]).then(([markdownFiles, repositoryInfo, presets]) => {
      setFiles(markdownFiles)
      setGithubUrl(repositoryInfo.githubUrl)
      setCustomPresets(presets)
      if (markdownFiles.length > 0) setActiveFile(markdownFiles[0])
      else if (repositoryInfo.githubUrl) setActiveFile(GITHUB_TAB_ID)
      else setActiveFile('explorer')
    })
  }, [project.path])

  // Poll for markdown files and custom presets
  useEffect(() => {
    const id = window.setInterval(() => {
      getMarkdownFiles(project.path).then((newFiles) => {
        setFiles((prev) => {
          const same =
            prev.length === newFiles.length && prev.every((f, i) => f === newFiles[i])
          if (same) return prev
          return newFiles
        })
        setActiveFile((prev) => {
          if (prev === null || prev === 'explorer' || prev === GITHUB_TAB_ID) {
            return newFiles.length > 0 ? newFiles[0] : prev
          }
          if (!newFiles.includes(prev)) {
            return newFiles.length > 0 ? newFiles[0] : 'explorer'
          }
          return prev
        })
      })
      getAgentPresets().then(setCustomPresets)
    }, 5000)
    return () => window.clearInterval(id)
  }, [project.path])

  useEffect(() => {
    if (!activeFile || activeFile === 'explorer' || activeFile === GITHUB_TAB_ID) return
    setContent(null)
    readMarkdownFile(activeFile).then((c) => {
      setContent(c ?? '*Could not read file.*')
    })
  }, [activeFile])

  useEffect(() => {
    setIsEditingTags(false)
    setTagInput(project.tags.map((tag) => tag.name).join(', '))
    setPrimaryTagInput(project.primaryTag ?? '')
  }, [project.path, project.tags, project.primaryTag])

  async function handleLaunch(target: LaunchTarget) {
    await setLastLaunched(project.path)
    launchProject(project, target)
  }

  async function handleCustomLaunch(preset: AgentPreset) {
    await setLastLaunched(project.path)
    launchProject(project, 'custom')
    setCustomAgentInfo(project.path, preset.label, preset.command)
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
      const override: ProjectTagOverride = { tags: [], primaryTag: null }
      const updated = await setTagOverride(project.path, override)
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
    const updated = await setTagOverride(project.path, override)
    setProjectTagOverride(project.path, updated.tagOverride)
    setIsEditingTags(false)
  }

  async function clearTagOverride() {
    const [updated, freshTags] = await Promise.all([
      setTagOverride(project.path, null),
      rescanProjectTags(project.path),
    ])
    setProjectDetectedTags(project.path, freshTags)
    setProjectTagOverride(project.path, updated.tagOverride)
    setTagInput(freshTags.map((tag) => tag.name).join(', '))
    setPrimaryTagInput(freshTags[0]?.name ?? '')
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
        <button
          onClick={() => handleLaunch('shell')}
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
          Shell
        </button>
        {customPresets.map((preset) => (
          <button
            key={preset.label}
            onClick={() => handleCustomLaunch(preset)}
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
            {preset.label}
          </button>
        ))}
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
        {githubUrl && (
          <button
            onClick={() => setActiveFile(GITHUB_TAB_ID)}
            style={{
              background: 'none',
              border: 'none',
              borderBottom: activeFile === GITHUB_TAB_ID ? '2px solid var(--accent)' : '2px solid transparent',
              color: activeFile === GITHUB_TAB_ID ? 'var(--text-primary)' : 'var(--text-secondary)',
              padding: '8px 14px',
              fontSize: 12,
              cursor: 'pointer',
              fontWeight: activeFile === GITHUB_TAB_ID ? 600 : 400,
              whiteSpace: 'nowrap',
            }}
          >
            GitHub
          </button>
        )}
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
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', overflowY: activeFile === 'explorer' || activeFile === GITHUB_TAB_ID ? 'hidden' : 'auto', padding: activeFile === 'explorer' ? 0 : activeFile === GITHUB_TAB_ID ? '16px' : '20px 24px' }}>
        {activeFile === 'explorer' && (
          <FileExplorerPane projectPath={project.path} />
        )}
        {activeFile === GITHUB_TAB_ID && githubUrl && (
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
        {activeFile !== 'explorer' && activeFile !== GITHUB_TAB_ID && content === null && activeFile && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</div>
        )}
        {activeFile !== 'explorer' && activeFile !== GITHUB_TAB_ID && content !== null && (
          <div className="markdown-body">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
              {content}
            </ReactMarkdown>
          </div>
        )}
        {files.length === 0 && !githubUrl && content === null && activeFile !== 'explorer' && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>
            No markdown files found in this project.
          </div>
        )}
      </div>
    </div>
  )
}
