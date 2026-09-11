import { useRef, useState, type DragEvent } from 'react';
import type { Entry } from '../bridge/types';
import { within } from './context';

const mime = 'application/x-remote-codex-entry';
export function useFileMoveDrag(
  context: string | null,
  move: (entry: Entry, parent: string) => Promise<void>,
  report: (error: unknown) => void,
) {
  const source = useRef<{ context: string; entry: Entry } | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const locate = (event: DragEvent): string | null => {
    if (!context || source.current?.context !== context || !event.dataTransfer.types.includes(mime)) return null;
    const row = (event.target as HTMLElement).closest<HTMLElement>('.file-row');
    const path = row ? row.dataset.moveTarget : '';
    if (path === undefined || (source.current.entry.directory && within(source.current.entry.path, path))) return null;
    return path;
  };
  return {
    target,
    start: (event: DragEvent, entry: Entry) => {
      if (!context) return;
      source.current = { context, entry };
      event.dataTransfer.setData(mime, entry.path);
      event.dataTransfer.effectAllowed = 'move';
    },
    end: () => {
      source.current = null;
      setTarget(null);
    },
    over: (event: DragEvent) => {
      const path = locate(event);
      setTarget(path);
      if (path !== null) {
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
      }
    },
    leave: (event: DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setTarget(null);
    },
    drop: (event: DragEvent) => {
      const path = locate(event),
        entry = source.current?.entry;
      source.current = null;
      setTarget(null);
      if (path === null || !entry) return;
      event.preventDefault();
      event.stopPropagation();
      void move(entry, path).catch(report);
    },
  };
}
