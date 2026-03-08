import { useEffect, useMemo, useState } from 'react'
import type { ReactElement } from 'react'

interface FileSystemEntry {
  name: string
  path: string
  isDirectory: boolean
}

type FilePreviewKind = 'text' | 'media' | 'unsupported' | 'tooLarge' | 'error'

interface FilePreview {
  kind: FilePreviewKind
  content?: string
  mimeType?: string
  size?: number
  message?: string
}

interface Props {
  projectPath: string
}

function humanBytes(size?: number): string {
  if (typeof size !== 'number') return ''
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
  return `${(size / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

function pathTail(filePath: string): string {
  return filePath.split('/').pop() ?? filePath
}

function buildDataUrl(preview: FilePreview): string | null {
  if (preview.kind !== 'media' || !preview.content || !preview.mimeType) return null
  return `data:${preview.mimeType};base64,${preview.content}`
}

export default function FileExplorerPane({ projectPath }: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})
  const [childrenByPath, setChildrenByPath] = useState<Record<string, FileSystemEntry[]>>({})
  const [loadingByPath, setLoadingByPath] = useState<Record<string, boolean>>({})
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [preview, setPreview] = useState<FilePreview | null>(null)
  const [loadingPreview, setLoadingPreview] = useState(false)

  useEffect(() => {
    setExpanded({ [projectPath]: true })
    setChildrenByPath({})
    setLoadingByPath({})
    setSelectedFile(null)
    setPreview(null)
  }, [projectPath])

  useEffect(() => {
    if (!expanded[projectPath] || childrenByPath[projectPath] || loadingByPath[projectPath]) return
    setLoadingByPath((value) => ({ ...value, [projectPath]: true }))
    window.sizzle.listDirectory(projectPath, projectPath).then((entries) => {
      setChildrenByPath((value) => ({ ...value, [projectPath]: entries }))
      setLoadingByPath((value) => ({ ...value, [projectPath]: false }))
    })
  }, [projectPath, expanded, childrenByPath, loadingByPath])

  useEffect(() => {
    if (!selectedFile) return
    setLoadingPreview(true)
    setPreview(null)
    window.sizzle.previewFile(projectPath, selectedFile).then((result) => {
      setPreview(result)
      setLoadingPreview(false)
    })
  }, [projectPath, selectedFile])

  const selectedLabel = useMemo(() => (selectedFile ? pathTail(selectedFile) : null), [selectedFile])
  const previewUrl = useMemo(() => buildDataUrl(preview ?? { kind: 'unsupported' }), [preview])

  function toggleDirectory(directoryPath: string) {
    const isExpanded = !!expanded[directoryPath]
    if (isExpanded) {
      setExpanded((value) => ({ ...value, [directoryPath]: false }))
      return
    }

    setExpanded((value) => ({ ...value, [directoryPath]: true }))
    if (!childrenByPath[directoryPath] && !loadingByPath[directoryPath]) {
      setLoadingByPath((value) => ({ ...value, [directoryPath]: true }))
      window.sizzle.listDirectory(projectPath, directoryPath).then((entries) => {
        setChildrenByPath((value) => ({ ...value, [directoryPath]: entries }))
        setLoadingByPath((value) => ({ ...value, [directoryPath]: false }))
      })
    }
  }

  function renderNodes(directoryPath: string, depth: number) {
    const entries = childrenByPath[directoryPath] ?? []
    const rows: ReactElement[] = []

    for (const entry of entries) {
      if (entry.isDirectory) {
        const isOpen = !!expanded[entry.path]
        rows.push(
          <button
            key={entry.path}
            onClick={() => toggleDirectory(entry.path)}
            style={{
              display: 'flex',
              alignItems: 'center',
              width: '100%',
              padding: `5px 8px 5px ${8 + depth * 14}px`,
              background: 'transparent',
              border: 'none',
              color: 'var(--text-secondary)',
              fontSize: 12,
              textAlign: 'left',
              cursor: 'pointer',
              fontWeight: 500,
              gap: 6,
            }}
            title={entry.path}
          >
            <span style={{ width: 10, color: 'var(--text-muted)' }}>{isOpen ? 'v' : '>'}</span>
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{entry.name}</span>
          </button>,
        )
        if (isOpen) {
          if (loadingByPath[entry.path]) {
            rows.push(
              <div
                key={`${entry.path}-loading`}
                style={{
                  padding: `2px 8px 4px ${28 + depth * 14}px`,
                  color: 'var(--text-muted)',
                  fontSize: 11,
                }}
              >
                Loading...
              </div>,
            )
          } else {
            rows.push(...renderNodes(entry.path, depth + 1))
          }
        }
        continue
      }

      const isSelected = selectedFile === entry.path
      rows.push(
        <button
          key={entry.path}
          onClick={() => setSelectedFile(entry.path)}
          style={{
            display: 'block',
            width: '100%',
            padding: `5px 8px 5px ${26 + depth * 14}px`,
            background: isSelected ? 'var(--bg-selected)' : 'transparent',
            border: 'none',
            color: isSelected ? 'var(--text-primary)' : 'var(--text-secondary)',
            fontSize: 12,
            textAlign: 'left',
            cursor: 'pointer',
            borderRadius: 4,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
          title={entry.path}
        >
          {entry.name}
        </button>,
      )
    }

    return rows
  }

  return (
    <div style={{ display: 'flex', flex: 1, minHeight: 0, overflow: 'hidden', borderTop: '1px solid var(--border)' }}>
      <div style={{
        width: 320,
        minWidth: 220,
        borderRight: '1px solid var(--border)',
        overflowY: 'auto',
        background: 'var(--bg-panel)',
      }}>
        <div style={{
          padding: '8px 10px',
          fontSize: 11,
          color: 'var(--text-muted)',
          textTransform: 'uppercase',
          letterSpacing: '0.05em',
          borderBottom: '1px solid var(--border)',
        }}>
          Files
        </div>
        {loadingByPath[projectPath] ? (
          <div style={{ padding: '10px', color: 'var(--text-muted)', fontSize: 12 }}>Loading...</div>
        ) : (
          <div style={{ padding: 6 }}>
            {renderNodes(projectPath, 0)}
          </div>
        )}
      </div>

      <div style={{ flex: 1, minWidth: 0, overflowY: 'auto', padding: '16px 18px' }}>
        {!selectedFile && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Select a file to preview.</div>
        )}

        {selectedFile && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, borderBottom: '1px solid var(--border)', paddingBottom: 8 }}>
              <div style={{ fontSize: 14, fontWeight: 600 }}>{selectedLabel}</div>
              <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{selectedFile}</div>
            </div>

            {loadingPreview && <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading...</div>}

            {!loadingPreview && preview?.kind === 'text' && (
              <pre style={{
                margin: 0,
                whiteSpace: 'pre-wrap',
                overflowWrap: 'anywhere',
                fontSize: 12,
                lineHeight: 1.5,
                fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
                background: '#111126',
                border: '1px solid var(--border)',
                borderRadius: 6,
                padding: '12px',
                color: 'var(--text-primary)',
              }}>
                {preview.content ?? ''}
              </pre>
            )}

            {!loadingPreview && preview?.kind === 'media' && previewUrl && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                {preview.mimeType?.startsWith('image/') && (
                  <img
                    src={previewUrl}
                    alt={selectedLabel ?? 'preview'}
                    style={{ maxWidth: '100%', maxHeight: '70vh', objectFit: 'contain', border: '1px solid var(--border)', borderRadius: 6 }}
                  />
                )}
                {preview.mimeType?.startsWith('video/') && (
                  <video src={previewUrl} controls style={{ maxWidth: '100%', maxHeight: '70vh', borderRadius: 6 }} />
                )}
                {preview.mimeType?.startsWith('audio/') && (
                  <audio src={previewUrl} controls style={{ width: 'min(100%, 560px)' }} />
                )}
                {preview.mimeType === 'application/pdf' && (
                  <iframe src={previewUrl} title={selectedLabel ?? 'pdf preview'} style={{ width: '100%', height: '75vh', border: '1px solid var(--border)', borderRadius: 6 }} />
                )}
                <div style={{ color: 'var(--text-muted)', fontSize: 11 }}>
                  {preview.mimeType}
                  {preview.size ? ` | ${humanBytes(preview.size)}` : ''}
                </div>
              </div>
            )}

            {!loadingPreview && (preview?.kind === 'unsupported' || preview?.kind === 'tooLarge' || preview?.kind === 'error') && (
              <div style={{ color: 'var(--text-secondary)', fontSize: 13 }}>
                {preview.message ?? 'This file type is not supported for preview.'}
                {preview.size ? ` (${humanBytes(preview.size)})` : ''}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
