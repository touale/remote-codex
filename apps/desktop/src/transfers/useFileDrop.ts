import { useEffect, useRef, useState } from 'react';
import { call, listen, onFileDrag } from '../bridge/client';
import type { Granted } from '../bridge/files';

function dropTarget(element: HTMLElement, x: number, y: number): string | null {
  const hit = document.elementFromPoint(x, y);
  if (!hit || !element.contains(hit)) return null;
  const row = hit.closest<HTMLElement>('[data-upload-target]');
  return row?.dataset.uploadTarget ?? '';
}
export function useFileDrop(
  enabled: boolean,
  upload: (parent: string, grant: Granted) => Promise<void>,
  report: (error: unknown) => void,
) {
  const area = useRef<HTMLElement>(null);
  const [target, setTarget] = useState<string | null>(null);
  const current = useRef({ enabled, upload, report });
  current.current = { enabled, upload, report };
  useEffect(() => {
    let disposed = false;
    let stopNative: (() => void) | undefined;
    const locate = (x: number, y: number) =>
      area.current ? dropTarget(area.current, x / devicePixelRatio, y / devicePixelRatio) : null;
    void onFileDrag((position) => setTarget(position ? locate(position.x, position.y) : null))
      .then((stop) => {
        if (disposed) stop();
        else stopNative = stop;
      })
      .catch(report);
    const stop = listen((event) => {
      if (event.kind !== 'files_dropped') return;
      const destination = locate(event.x, event.y);
      setTarget(null);
      if (destination === null || !current.current.enabled) {
        void call('transfer_discard_grant', { token: event.token }).catch(current.current.report);
        if (destination !== null) current.current.report({ message: 'Files are not ready for uploads yet.' });
      } else
        void current.current
          .upload(destination, { token: event.token, names: event.names })
          .catch(current.current.report);
    });
    return () => {
      disposed = true;
      stopNative?.();
      stop();
    };
  }, [report]);
  return { area, target };
}
