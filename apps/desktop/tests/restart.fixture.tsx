import { useEffect, useState } from 'react';
import type { useApplication } from '../src/app/useApplication';
import { useResourceActions } from '../src/app/useResourceActions';
import { DraftStore } from '../src/chat/drafts/store';
import { useChats } from '../src/chat/useChats';
import { useFiles } from '../src/files/useFiles';
import { useFileActions } from '../src/files/useFileActions';
import { useDialog } from '../src/ui/useDialog';
import { fileKey } from '../src/files/tabs';

export function RestartFixture({ app }: { app: ReturnType<typeof useApplication> }) {
  const dialog = useDialog();
  const [error, setError] = useState('');
  const [drafts] = useState(() => new DraftStore());
  const files = useFiles();
  const chats = useChats(app.report, dialog.ask);
  const report = (error: unknown) => setError(String((error as Error).message ?? error));
  const context = { id: 'fixture', kind: 'workspace' as const, server: 'fixture', path: '/fixture' };
  const actions = useFileActions(
    context,
    files,
    dialog.ask,
    () => {},
    () => {},
  );
  const resources = useResourceActions(
    { ...app, report },
    { target: null, workspace: null, forgetWorkspace: () => {}, drafts },
    chats,
    files,
    dialog.ask,
    { tabs: [], close: async () => {} },
    actions,
  );
  useEffect(() => {
    const previous = app.closeHandler.current;
    app.closeHandler.current = () => void resources.closeWindow();
    return () => {
      app.closeHandler.current = previous;
    };
  }, [app.closeHandler, resources.closeWindow]);
  return (
    <>
      <button
        onClick={() =>
          void files
            .open(context.id, 'note.txt', context.server, context.path)
            .then(() => {
              files.update(fileKey(context.server, context.path, 'note.txt'), { text: 'unsaved edit' });
            })
            .catch(report)
        }
      >
        Prepare unsaved file
      </button>
      <button
        onClick={() => {
          const key = drafts.ensure({ server: 'fixture', path: '/fixture' });
          drafts.update(key, (draft) => ({ ...draft, text: 'unsent message' }));
        }}
      >
        Prepare unsent message
      </button>
      <button onClick={() => drafts.forget('fixture')}>Clear unsent message</button>
      <output id="restart-dirty">{files.all().filter((file) => file.text !== file.original).length}</output>
      <output id="restart-error">{error}</output>
      {dialog.dialog}
    </>
  );
}
