import { ipcMain, WebContents } from 'electron'
import chokidar from 'chokidar'

interface WatchEntry {
  watcher: chokidar.FSWatcher
  debounceTimer: ReturnType<typeof setTimeout> | null
  sender: WebContents
}

const watchers = new Map<string, WatchEntry>()

const DEBOUNCE_MS = 400

function stopWatcher(projectPath: string): void {
  const entry = watchers.get(projectPath)
  if (!entry) return
  if (entry.debounceTimer) clearTimeout(entry.debounceTimer)
  void entry.watcher.close()
  watchers.delete(projectPath)
}

export function registerGitWatcherHandlers(): void {
  ipcMain.handle('git:watch', (event, projectPath: string) => {
    stopWatcher(projectPath)

    const sender = event.sender

    const watcher = chokidar.watch(projectPath, {
      // Skip git object store and logs — they change constantly during operations
      // but don't affect what `git status` reports
      ignored: /[/\\]\.git[/\\](objects|logs)[/\\]/,
      ignoreInitial: true,
      persistent: true,
      awaitWriteFinish: { stabilityThreshold: 100, pollInterval: 50 },
    })

    const entry: WatchEntry = { watcher, debounceTimer: null, sender }
    watchers.set(projectPath, entry)

    const notify = () => {
      if (entry.debounceTimer) clearTimeout(entry.debounceTimer)
      entry.debounceTimer = setTimeout(() => {
        entry.debounceTimer = null
        if (!sender.isDestroyed()) {
          sender.send('git:changed', projectPath)
        }
      }, DEBOUNCE_MS)
    }

    watcher.on('add', notify)
    watcher.on('change', notify)
    watcher.on('unlink', notify)
    watcher.on('addDir', notify)
    watcher.on('unlinkDir', notify)
  })

  ipcMain.handle('git:unwatch', (_event, projectPath: string) => {
    stopWatcher(projectPath)
  })
}
