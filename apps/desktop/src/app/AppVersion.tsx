import { CircleArrowUp, RotateCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { attach, call, failure, listen } from '../bridge/client';
import type { UpdateSnapshot } from '../bridge/updates';
import { IconButton } from '../ui/controls';
import styles from './Sidebar.module.css';

export function AppVersion({ onOpen }: { onOpen: () => void }) {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    let revision = 0;
    const refresh = async () => {
      if (!active) return;
      const current = ++revision;
      try {
        const value = await call('update_status', {});
        if (active && current === revision) {
          setSnapshot(value);
          setError('');
        }
      } catch (error) {
        if (active && current === revision) setError(failure(error).message);
      }
    };
    const stop = listen((event) => {
      if (event.kind === 'updates_changed') void refresh();
    });
    const focus = () => void refresh();
    window.addEventListener('focus', focus);
    void attach()
      .then(refresh)
      .catch((error) => {
        if (active) setError(failure(error).message);
      });
    return () => {
      active = false;
      stop();
      window.removeEventListener('focus', focus);
    };
  }, []);
  const available = snapshot?.available;
  const label = available
    ? `Update available${snapshot.latest ? `: v${snapshot.latest.version}` : ''}`
    : snapshot?.restart_required
      ? `Restart to use v${snapshot.installed_version}`
      : null;
  return (
    <div className={styles.version}>
      <small
        aria-label="Remote Codex version"
        title={error || (snapshot ? `Remote Codex v${snapshot.current_version}` : 'Loading app version…')}
      >
        {snapshot ? `v${snapshot.current_version}` : error ? '—' : '…'}
      </small>
      {label && (
        <IconButton label={label} onClick={onOpen}>
          {available ? <CircleArrowUp size={14} /> : <RotateCw size={14} />}
        </IconButton>
      )}
    </div>
  );
}
