import { Archive, RotateCw, Search } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { call, failure, listen } from '../bridge/client';
import type { CachedSession } from '../bridge/types';
import { fullTime } from '../chat/time';
import { Empty, ErrorText, IconButton } from '../ui/controls';
import styles from './ArchivedSessions.module.css';

export function ArchivedSessions() {
  const [sessions, setSessions] = useState<CachedSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [restoring, setRestoring] = useState(new Set<string>());
  const [error, setError] = useState('');
  const [query, setQuery] = useState('');
  const revision = useRef(0);
  const pending = useRef(new Set<string>());
  const refresh = useCallback(async () => {
    const request = ++revision.current;
    setLoading(true);
    try {
      const catalog = await call('catalog', { archived: true });
      if (request === revision.current) {
        setSessions(catalog.sessions);
        setError('');
      }
    } catch (error) {
      if (request === revision.current) setError(failure(error).message);
    } finally {
      if (request === revision.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    const stop = listen((event) => {
      if (event.kind === 'catalog_changed') void refresh();
    });
    return () => {
      ++revision.current;
      stop();
    };
  }, [refresh]);
  const restore = async (id: string) => {
    if (pending.current.has(id)) return;
    pending.current.add(id);
    setRestoring(new Set(pending.current));
    try {
      await call('session_metadata', { id, name: null, archived: false });
      await refresh();
    } catch (error) {
      setError(failure(error).message);
    } finally {
      pending.current.delete(id);
      setRestoring(new Set(pending.current));
    }
  };
  const matches = sessions.filter((item) =>
    `${item.session.title} ${item.server} ${item.session.cwd}`.toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <section className={styles.root} aria-label="Archived sessions">
      <div className={styles.heading}>
        <h3>Archived sessions</h3>
        <IconButton label="Refresh archived sessions" disabled={loading} onClick={() => void refresh()}>
          <RotateCw size={14} className={loading ? 'spinning' : ''} />
        </IconButton>
      </div>
      <p className="settings-caption">
        Restore a session to show it in Workspaces. Restoring does not open or connect it.
      </p>
      <label className={styles.search}>
        <Search size={14} />
        <input
          aria-label="Search archived sessions"
          placeholder="Search sessions, servers or workspaces"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <ErrorText message={error} />
      <div className={styles.list} aria-busy={loading}>
        {!sessions.length && loading ? (
          <p className="muted" role="status">
            Loading archived sessions…
          </p>
        ) : !matches.length ? (
          <Empty
            title={query ? 'No matching sessions' : 'No archived sessions'}
            detail={query ? 'Try a different search.' : 'Sessions you archive will appear here.'}
          />
        ) : (
          matches.map(({ session, server }) => (
            <div className={styles.row} key={session.id} data-archived-session={session.id}>
              <Archive size={15} />
              <div className={styles.details}>
                <strong title={session.title}>{session.title || 'Untitled session'}</strong>
                <small title={`${server} · ${session.cwd}`}>
                  {server} · {session.cwd}
                </small>
                <time title={fullTime(session.updated_at)} dateTime={new Date(session.updated_at * 1000).toISOString()}>
                  {new Date(session.updated_at * 1000).toLocaleDateString([], {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                  })}
                </time>
              </div>
              <button disabled={restoring.has(session.id)} onClick={() => void restore(session.id)}>
                {restoring.has(session.id) ? 'Restoring…' : 'Restore'}
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
