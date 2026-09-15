import { useState } from 'react';
import type { CurrentWorkspace } from '../bridge/files';
import type { Catalog, Preferences } from '../bridge/types';
import { SelectField } from '../ui/SelectField';

import { call, failure, operationId } from '../bridge/client';
import { ErrorText, Modal } from '../ui/controls';
import { useDialog } from '../ui/useDialog';
import { ArchivedSessions } from './ArchivedSessions';
import { CodexSettings } from './CodexSettings';
import { GeneralSettings } from './GeneralSettings';
import { UpdatesSettings } from './UpdatesSettings';
import { McpSettings } from './McpSettings';

export function SettingsDialog({
  preferences,
  onPreferences,
  onClose,
  catalog,
  workspace,
  session,
}: {
  preferences: Preferences;
  onPreferences: (value: Partial<Preferences>) => void;
  onClose: () => void;
  catalog: Catalog;
  workspace: CurrentWorkspace | null;
  session: string | null;
}) {
  const [tab, setTab] = useState('General');
  const [bound, setBound] = useState(workspace);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const dialog = useDialog();
  const leave = async () =>
    !dirty ||
    Boolean(
      await dialog.ask({
        title: 'Discard MCP changes?',
        message: 'Save your configuration before leaving, or discard these changes.',
        choices: ['Discard changes'],
      }),
    );
  const close = () =>
    void leave().then((ok) => {
      if (ok) onClose();
    });
  return (
    <>
      <Modal title="Settings" className="settings-modal" onClose={close} wide>
        <div className="settings-layout">
          <nav className="settings-nav" aria-label="Settings sections">
            {['General', 'Codex', 'MCP', 'Sessions', 'Updates'].map((name) => (
              <button
                key={name}
                className={tab === name ? 'selected' : ''}
                aria-current={tab === name ? 'page' : undefined}
                disabled={busy}
                onClick={() => {
                  if (tab !== name)
                    void leave().then((ok) => {
                      if (ok) {
                        setDirty(false);
                        setTab(name);
                      }
                    });
                }}
              >
                {name}
              </button>
            ))}
          </nav>
          <div className="settings-content">
            {tab === 'General' && <GeneralSettings report={(error) => setError(failure(error).message)} />}
            {tab === 'Updates' && <UpdatesSettings />}
            {tab === 'Codex' && <CodexSettings preferences={preferences} onPreferences={onPreferences} />}
            {tab === 'Sessions' && <ArchivedSessions />}
            {tab === 'MCP' && (
              <>
                <label className="field">
                  <span>Workspace</span>
                  <SelectField
                    label="MCP workspace"
                    placeholder="Choose a workspace"
                    options={catalog.workspaces.map((w) => ({
                      value: JSON.stringify([w.server, w.path]),
                      label: `${w.server} · ${w.path}`,
                    }))}
                    disabled={busy}
                    value={bound ? JSON.stringify([bound.server, bound.path]) : ''}
                    onValueChange={(value) => {
                      void (async () => {
                        if (!value || !(await leave())) return;
                        setBusy(true);
                        setError('');
                        try {
                          const [server, path] = JSON.parse(value);
                          const next = await call('workspace_open', {
                            operationId: operationId(),
                            server,
                            path,
                          });
                          setDirty(false);
                          setBound(next);
                        } catch (e) {
                          setError(failure(e).message);
                        } finally {
                          setBusy(false);
                        }
                      })();
                    }}
                  />
                </label>
                {bound ? (
                  <McpSettings
                    key={bound.id}
                    workspace={bound.id}
                    session={bound.id === workspace?.id ? session : null}
                    onDirty={setDirty}
                  />
                ) : (
                  <p className="muted">
                    Choose a workspace to manage its remote MCP servers. Local MCP is managed by Codex.
                  </p>
                )}
              </>
            )}
            <ErrorText message={error} />
          </div>
        </div>
      </Modal>
      {dialog.dialog}
    </>
  );
}
