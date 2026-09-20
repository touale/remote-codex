import { visibleIn } from '../files/context';
import EditorPane from '../files/Editor';
import type { useFileActions } from '../files/useFileActions';
import type { useFiles } from '../files/useFiles';
import { TerminalPanel } from '../terminal/TerminalPanel';
import type { useTerminals } from '../terminal/useTerminals';
import { ResizeHandle } from '../ui/ResizeHandle';
import styles from './App.module.css';
import type { useApplication } from './useApplication';
import type { useNavigation } from './useNavigation';
interface Props {
  app: Pick<ReturnType<typeof useApplication>, 'preferences' | 'report' | 'changePreferences' | 'dark' | 'setError'>;
  nav: Pick<ReturnType<typeof useNavigation>, 'fileContext' | 'workspace' | 'target' | 'serverHome'>;
  files: Pick<
    ReturnType<typeof useFiles>,
    | 'tabs'
    | 'open'
    | 'setPreviewView'
    | 'selected'
    | 'diff'
    | 'setDiff'
    | 'select'
    | 'retry'
    | 'update'
    | 'discard'
    | 'moveWindow'
    | 'openDiffWindow'
  >;
  fileActions: Pick<ReturnType<typeof useFileActions>, 'save' | 'close'>;
  terminals: Pick<ReturnType<typeof useTerminals>, 'tabs' | 'selected' | 'select' | 'open' | 'close' | 'ended'>;
}
export function hasEditorContent(nav: Props['nav'], files: Props['files']) {
  return Boolean(
    nav.fileContext &&
    (files.tabs.some((b) => visibleIn(nav.fileContext, b)) || files.diff?.workspace === nav.fileContext.id),
  );
}

export function EditorArea({ app, nav, files, fileActions }: Pick<Props, 'app' | 'nav' | 'files' | 'fileActions'>) {
  const prefs = app.preferences;
  const run = (promise: Promise<unknown>) => void promise.catch(app.report);
  return (
    <>
      {prefs.editor_visible && hasEditorContent(nav, files) && (
        <>
          <ResizeHandle
            axis="x"
            reverse
            label="Resize editor"
            value={prefs.editor_width}
            min={280}
            max={1000}
            onChange={(editor_width) => app.changePreferences({ editor_width })}
          />
          <div className={styles.editor} style={{ width: prefs.editor_width }}>
            <EditorPane
              context={nav.fileContext}
              tabs={files.tabs}
              selected={nav.fileContext ? files.selected[nav.fileContext.id] : undefined}
              dark={app.dark}
              diff={files.diff?.workspace === nav.fileContext?.id ? files.diff : null}
              onSelect={(key) => {
                if (nav.fileContext) files.select(nav.fileContext.id, key);
              }}
              onRetry={(key) => run(files.retry(key))}
              onClose={(key) => run(fileActions.close(key))}
              onSave={(key, resolve) => run(fileActions.save(key, resolve))}
              onMode={(key, mode, view) => files.update(key, { mode, ...(view ? { view } : {}) })}
              onPreviewView={files.setPreviewView}
              onOpenFile={files.open}
              onChange={(key, text) => files.update(key, { text })}
              onHide={() => app.changePreferences({ editor_visible: false })}
              onCloseDiff={() => files.setDiff(null)}
              onWindow={(key) => run(files.moveWindow(key))}
              onDiffWindow={() => {
                if (nav.fileContext && files.diff) run(files.openDiffWindow(nav.fileContext, files.diff));
              }}
              onDismissConflict={(key) => files.update(key, { conflict: undefined })}
              onDiscardConflict={(key) => {
                files.discard(key);
                app.setError('');
              }}
            />
          </div>
        </>
      )}
    </>
  );
}
export function TerminalArea({ app, nav, terminals }: Pick<Props, 'app' | 'nav' | 'terminals'>) {
  const prefs = app.preferences;
  const run = (promise: Promise<unknown>) => void promise.catch(app.report);
  const server = nav.workspace?.server ?? nav.target?.server ?? nav.serverHome?.name ?? null;
  const target = nav.fileContext ?? (server ? { server } : null);
  const visible = prefs.terminal_visible && Boolean(server && terminals.tabs.some((t) => t.server === server));
  return (
    <div style={{ display: visible ? 'contents' : 'none' }}>
      {visible && (
        <ResizeHandle
          axis="y"
          reverse
          label="Resize terminal"
          value={prefs.terminal_height}
          min={100}
          max={700}
          onChange={(terminal_height) => app.changePreferences({ terminal_height })}
        />
      )}
      <div
        className={styles.terminals}
        style={{
          height: prefs.terminal_height,
          minHeight: 100,
        }}
      >
        <TerminalPanel
          tabs={terminals.tabs}
          visible={visible}
          server={server}
          selected={server ? (terminals.selected[server] ?? null) : null}
          dark={app.dark}
          onSelect={(id) => {
            if (server) terminals.select(server, id);
          }}
          onNew={() => {
            if (target) run(terminals.open(target, true));
          }}
          onClose={(id) => run(terminals.close(id))}
          onHide={() => app.changePreferences({ terminal_visible: false })}
          onEnded={terminals.ended}
          report={app.report}
        />
      </div>
    </div>
  );
}
