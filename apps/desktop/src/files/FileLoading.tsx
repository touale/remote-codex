export function FileTreeSkeleton({ depth = 0, path = '' }: { depth?: number; path?: string }) {
  return (
    <div
      className="file-tree-skeleton"
      role="status"
      aria-label={path ? `Loading ${path}` : 'Loading files'}
      aria-busy="true"
      style={{ paddingLeft: 30 + depth * 14 }}
    >
      {(depth ? [62, 78, 51] : [68, 83, 57, 76, 62, 88]).map((width, index) => (
        <div key={index} aria-hidden="true">
          <i />
          <span style={{ width: `${width}%` }} />
        </div>
      ))}
    </div>
  );
}

export function EditorSkeleton() {
  return (
    <div className="editor-skeleton" role="status" aria-label="Loading file content" aria-busy="true">
      {[52, 71, 39, 64, 47, 76, 58, 33, 62, 44].map((width, index) => (
        <div key={index} aria-hidden="true">
          <i />
          <span style={{ width: `${width}%` }} />
        </div>
      ))}
    </div>
  );
}
