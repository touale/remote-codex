import { useState } from 'react';
import { FileTree } from '../src/files/FileTree';

const noop = () => {};
export function FileTreeFixture() {
  const [root, setRoot] = useState<string | null>('/workspace/project');
  const [error, setError] = useState('');
  return (
    <div style={{ width: 360, minHeight: 0 }}>
      <button onClick={() => setRoot('/')}>Browse root</button>
      <button onClick={() => setRoot(null)}>Clear root</button>
      <output id="file-tree-error">{error}</output>
      <FileTree
        onNewSession={() => {}}
        context="fixture"
        server="fixture"
        root={root}
        cacheKey="copy-path-fixture"
        loading={false}
        error=""
        onRetry={noop}
        collapsed={false}
        onCollapsed={noop}
        refresh={0}
        onOpen={noop}
        onWindow={noop}
        onCreate={async () => false}
        onRename={noop}
        onMove={async () => {}}
        onRemove={noop}
        onUpload={noop}
        onDownload={noop}
        onDrop={async () => {}}
        report={(error) => setError(error instanceof Error ? error.message : String(error))}
      />
    </div>
  );
}
