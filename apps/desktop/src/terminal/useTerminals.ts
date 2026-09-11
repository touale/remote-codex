import { useCallback, useRef, useState } from 'react';
import { call, operationId } from '../bridge/client';
import type { CurrentWorkspace } from '../bridge/files';

import type { TerminalTab } from './TerminalPanel';

type TerminalContext = CurrentWorkspace | { server: string };

export function useTerminals({
  visible,
  onVisibility,
  finishOperation,
}: {
  visible: boolean;
  onVisibility: (visible: boolean) => void;
  finishOperation: (id: string) => void;
}) {
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [selected, setSelected] = useState<Record<string, string>>({});
  const current = useRef(tabs);
  current.current = tabs;
  const pending = useRef(new Map<string, Promise<void>>());
  const open = (workspace: TerminalContext, force = false): Promise<void> => {
    const running = pending.current.get(workspace.server);
    if (running) return running;
    onVisibility(true);
    const existing = current.current.filter((t) => t.server === workspace.server && !t.closed);
    if (!force && existing.length) {
      setSelected((previous) => ({
        ...previous,
        [workspace.server]: existing.find((t) => t.id === previous[workspace.server])?.id ?? existing.at(-1)!.id,
      }));
      return Promise.resolve();
    }
    const task = (async () => {
      const operation = operationId();
      try {
        const result = await call('terminal_open', {
          operationId: operation,
          target:
            'id' in workspace
              ? { kind: 'workspace', workspace: workspace.id }
              : { kind: 'server', server: workspace.server },
          columns: 100,
          rows: 20,
        });
        const tab: TerminalTab = {
          ...result,
          workspace: 'id' in workspace ? workspace.id : null,
          title: result.path,
          closed: false,
        };
        current.current = [...current.current, tab];
        setTabs(current.current);
        setSelected((previous) => ({ ...previous, [workspace.server]: result.id }));
      } finally {
        finishOperation(operation);
        pending.current.delete(workspace.server);
      }
    })();
    pending.current.set(workspace.server, task);
    return task;
  };
  const close = async (id: string) => {
    await call('terminal_close', { id });
    current.current = current.current.filter((t) => t.id !== id);
    setTabs(current.current);
  };
  const ended = useCallback((id: string) => {
    current.current = current.current.map((t) => (t.id === id ? { ...t, closed: true } : t));
    setTabs(current.current);
  }, []);
  return {
    tabs,
    selected,
    open,
    close,
    ended,
    select: (workspace: string, id: string) => setSelected((previous) => ({ ...previous, [workspace]: id })),
    toggle: (workspace: TerminalContext) => {
      if (visible && current.current.some((t) => t.server === workspace.server && !t.closed)) {
        onVisibility(false);
        return Promise.resolve();
      }
      return open(workspace);
    },
  };
}
