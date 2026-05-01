import type { ApiManifest } from './api-manifest'

// Source of truth for the API surface.
// Must be kept in sync with the Rust #[tauri::command] functions
// and the invoke() calls in src/renderer/api.ts.
export const API_MANIFEST: ApiManifest = {
  format: 1,
  commands: [
    { name: 'get_api_manifest', args: [] },
    { name: 'scan_projects', args: [] },
    { name: 'rescan_project_tags', args: ['projectPath'] },
    { name: 'get_scan_settings', args: [] },
    { name: 'set_scan_settings', args: ['settings'] },
    { name: 'add_ignore_root', args: ['rootPath'] },
    { name: 'get_markdown_files', args: ['projectPath'] },
    { name: 'read_markdown_file', args: ['filePath'] },
    { name: 'list_directory', args: ['projectPath', 'directoryPath'] },
    { name: 'preview_file', args: ['projectPath', 'filePath'] },
    { name: 'get_project_repository_info', args: ['projectPath'] },
    { name: 'get_git_status', args: ['projectPath'] },
    { name: 'get_metadata', args: ['projectPath'] },
    { name: 'get_all_metadata', args: [] },
    { name: 'set_last_launched', args: ['projectPath'] },
    { name: 'set_tag_override', args: ['projectPath', 'overrideVal'] },
    { name: 'set_project_marker', args: ['projectPath', 'marker'] },
    { name: 'move_rename_project', args: ['oldPath', 'newPath'] },
    { name: 'claude_has_session', args: ['projectPath'] },
    { name: 'get_default_shell', args: [] },
    { name: 'pty_create', args: ['id', 'cwd', 'command', 'args'] },
    { name: 'pty_write', args: ['id', 'data'] },
    { name: 'pty_resize', args: ['id', 'cols', 'rows'] },
    { name: 'pty_detach', args: ['id'] },
    { name: 'pty_kill', args: ['id'] },
    { name: 'pty_list_sessions', args: [] },
    { name: 'get_agent_presets', args: [] },
    { name: 'set_agent_presets', args: ['presets'] },
  ],
  events: [
    { name: 'pty:data' },
    { name: 'pty:exit' },
  ],
}
