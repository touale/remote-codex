import { useEffect, useState } from 'react';
import { StatusBar } from '../src/app/StatusBar';
import type { Transfer } from '../src/bridge/files';
import { DraftStore } from '../src/chat/drafts/store';
import { TransferButton } from '../src/transfers/TransferButton';
import { accept, transferStore } from '../src/transfers/store';

const unit = 1048576;
const upload: Transfer = {
  id: 'upload',
  name: 'assets',
  server: 'fixture',
  workspace: '/project',
  direction: 'upload',
  destination: '',
  status: 'running',
  active: true,
  owned: true,
  bytes: 25 * unit,
  total: 100 * unit,
  files: 1,
  completed: 0,
  skipped: 0,
  message: null,
  error_code: null,
  conflict: null,
};
const done = async () => {};
const actions = { pick: done, upload: done, download: done, remove: done, resume: done, pause: done, cancel: done };
const report = (error: unknown) => {
  throw error;
};

export function TransferProgressFixture() {
  const [drafts] = useState(() => new DraftStore());
  const [tasks, setTasks] = useState([upload]);
  const [width, setWidth] = useState(720);
  useEffect(() => {
    tasks.forEach(accept);
  }, [tasks]);
  useEffect(
    () => () => {
      void transferStore.refresh().catch(report);
    },
    [],
  );
  return (
    <div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        <button
          onClick={() =>
            setTasks((tasks) => [
              ...tasks,
              { ...upload, id: 'download', direction: 'download', bytes: 150 * unit, total: 300 * unit },
            ])
          }
        >
          Add download
        </button>
        <button
          onClick={() => setTasks((tasks) => tasks.map((t) => (t.id === 'upload' ? { ...t, bytes: 50 * unit } : t)))}
        >
          Advance transfer
        </button>
        <button onClick={() => setWidth(360)}>Narrow status bar</button>
        {(['preparing', 'reconnecting', 'paused', 'conflict', 'completed'] as const).map((status) => (
          <button
            key={status}
            onClick={() =>
              setTasks((tasks) =>
                tasks.map((t) => ({ ...t, status, active: status === 'preparing' || status === 'reconnecting' })),
              )
            }
          >
            {status}
          </button>
        ))}
      </div>
      <div id="transfer-status-fixture" style={{ width, maxWidth: '100%', marginTop: 20 }}>
        <StatusBar
          drafts={drafts}
          draftKey={null}
          busy
          ready
          workspace={false}
          error=""
          authenticating={false}
          report={report}
          progress={{
            connection: { type: 'transfer', data: { kind: 'upload', transferred_bytes: 1, total_bytes: 4 } },
          }}
          transfers={<TransferButton actions={actions} report={report} />}
        />
      </div>
    </div>
  );
}
