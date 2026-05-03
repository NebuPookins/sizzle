import { useState, useEffect, useCallback, useRef, useImperativeHandle, forwardRef } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import Link from '@tiptap/extension-link'
import TaskList from '@tiptap/extension-task-list'
import TaskItem from '@tiptap/extension-task-item'
import { createLowlight, common } from 'lowlight'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { marked } from 'marked'
import TurndownService from 'turndown'
import { gfm } from 'turndown-plugin-gfm'

const lowlight = createLowlight(common)

const turndown = new TurndownService({
  headingStyle: 'atx',
  codeBlockStyle: 'fenced',
  bulletListMarker: '-',
})
turndown.use(gfm)

export interface RichMarkdownEditorHandle {
  isDirty: () => boolean
}

interface Props {
  filePath: string | null
  content: string | null
  onSave?: (filePath: string, content: string) => Promise<void>
}

const ToolbarButton = ({ onClick, isActive, label, title }: { onClick: () => void; isActive?: boolean; label: string; title: string }) => (
  <button
    onClick={onClick}
    title={title}
    className={`editor-toolbar-btn${isActive ? ' is-active' : ''}`}
    type="button"
  >
    {label}
  </button>
)

const ToolbarSeparator = () => <div className="editor-toolbar-separator" />

const RichMarkdownEditor = forwardRef<RichMarkdownEditorHandle, Props>(
  ({ filePath, content, onSave }, ref) => {
    const [isEditing, setIsEditing] = useState(false)
    const [dirty, setDirty] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const [saving, setSaving] = useState(false)
    const lastSavedRef = useRef<string | null>(null)
    const isEditingRef = useRef(false)
    const initialLoadRef = useRef(false)

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3, 4, 5, 6] },
        }),
        Placeholder.configure({
          placeholder: 'Start writing…',
        }),
        CodeBlockLowlight.configure({ lowlight }),
        Link.configure({ openOnClick: false }),
        TaskList,
        TaskItem.configure({ nested: true }),
      ],
      onUpdate: () => {
        if (isEditingRef.current && !initialLoadRef.current) {
          setDirty(true)
        }
      },
    })

    // Keep ref in sync for onUpdate closure
    useEffect(() => { isEditingRef.current = isEditing }, [isEditing])

    useImperativeHandle(ref, () => ({
      isDirty: () => dirty,
    }), [dirty])

    // Reset dirty/error when content changes (new file)
    useEffect(() => {
      setDirty(false)
      setError(null)
    }, [filePath])

    // Load content into editor when entering edit mode
    useEffect(() => {
      if (!editor || !content || !isEditing) return
      initialLoadRef.current = true
      const html = marked.parse(content)
      editor.commands.setContent(html as string)
      lastSavedRef.current = content
      setTimeout(() => { initialLoadRef.current = false }, 0)
      setDirty(false)
    }, [editor, content, isEditing])

    // Sync editor editable state
    useEffect(() => {
      if (!editor) return
      editor.setEditable(isEditing)
    }, [editor, isEditing])

    const handleSave = useCallback(async () => {
      if (!editor || !filePath || !onSave) return
      setSaving(true)
      setError(null)
      try {
        const html = editor.getHTML()
        const markdown = turndown.turndown(html)
        await onSave(filePath, markdown)
        lastSavedRef.current = markdown
        setDirty(false)
        setIsEditing(false)
      } catch (e: any) {
        setError(e?.message || 'Failed to save file')
      } finally {
        setSaving(false)
      }
    }, [editor, filePath, onSave])

    const handleCancel = useCallback(() => {
      if (!editor || !content) return
      const html = marked.parse(content)
      editor.commands.setContent(html as string)
      editor.commands.focus()
      setDirty(false)
      setError(null)
      setIsEditing(false)
    }, [editor, content])

    // Ctrl+S / Cmd+S
    useEffect(() => {
      if (!isEditing) return
      const handler = (e: KeyboardEvent) => {
        if ((e.ctrlKey || e.metaKey) && e.key === 's') {
          e.preventDefault()
          handleSave()
        }
      }
      window.addEventListener('keydown', handler)
      return () => window.removeEventListener('keydown', handler)
    }, [isEditing, handleSave])

    // Confirm before closing with unsaved changes
    useEffect(() => {
      if (!isEditing) return
      const handler = (e: BeforeUnloadEvent) => {
        if (dirty) {
          e.preventDefault()
        }
      }
      window.addEventListener('beforeunload', handler)
      return () => window.removeEventListener('beforeunload', handler)
    }, [isEditing, dirty])

    // View mode
    if (!isEditing) {
      return (
        <div style={{ position: 'relative', height: '100%', display: 'flex', flexDirection: 'column' }}>
          {content !== null && (
            <>
              <div className="markdown-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
                  {content}
                </ReactMarkdown>
              </div>
              <button
                onClick={() => setIsEditing(true)}
                className="editor-edit-btn"
                type="button"
              >
                Edit
              </button>
            </>
          )}
        </div>
      )
    }

    // Edit mode
    return (
      <div className="rich-editor">
        <div className="editor-toolbar">
          <ToolbarButton onClick={() => editor?.chain().focus().toggleBold().run()} isActive={editor?.isActive('bold')} label="B" title="Bold (Ctrl+B)" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleItalic().run()} isActive={editor?.isActive('italic')} label="I" title="Italic (Ctrl+I)" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleStrike().run()} isActive={editor?.isActive('strike')} label="S" title="Strike" />
          <ToolbarSeparator />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleHeading({ level: 1 }).run()} isActive={editor?.isActive('heading', { level: 1 })} label="H1" title="Heading 1" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()} isActive={editor?.isActive('heading', { level: 2 })} label="H2" title="Heading 2" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()} isActive={editor?.isActive('heading', { level: 3 })} label="H3" title="Heading 3" />
          <ToolbarSeparator />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleBulletList().run()} isActive={editor?.isActive('bulletList')} label="•" title="Bullet List" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleOrderedList().run()} isActive={editor?.isActive('orderedList')} label="1." title="Ordered List" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleTaskList().run()} isActive={editor?.isActive('taskList')} label="☑" title="Task List" />
          <ToolbarSeparator />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleBlockquote().run()} isActive={editor?.isActive('blockquote')} label="❝" title="Blockquote" />
          <ToolbarButton onClick={() => editor?.chain().focus().toggleCodeBlock().run()} isActive={editor?.isActive('codeBlock')} label="<>" title="Code Block" />
          <ToolbarButton
            onClick={() => {
              const url = window.prompt('URL:')
              if (url) {
                editor?.chain().focus().setLink({ href: url }).run()
              }
            }}
            isActive={editor?.isActive('link')}
            label="🔗"
            title="Link"
          />
          <ToolbarSeparator />
          <ToolbarButton onClick={() => editor?.chain().focus().undo().run()} label="↩" title="Undo" />
          <ToolbarButton onClick={() => editor?.chain().focus().redo().run()} label="↪" title="Redo" />
        </div>

        <div className="editor-content-area">
          <EditorContent editor={editor} />
        </div>

        <div className="editor-actions">
          {dirty && <span className="dirty-indicator">Unsaved changes</span>}
          {error && <span className="error-text">{error}</span>}
          <div style={{ flex: 1 }} />
          <button className="editor-cancel-btn" onClick={handleCancel} type="button">Cancel</button>
          <button className="editor-save-btn" onClick={handleSave} disabled={saving} type="button">
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    )
  }
)

RichMarkdownEditor.displayName = 'RichMarkdownEditor'

export default RichMarkdownEditor
