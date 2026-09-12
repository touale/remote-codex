import { useCallback, useEffect, useRef, useState } from 'react';
import { attach, call, failure, listen } from '../bridge/client';
import type { AuthPrompt, Catalog, Preferences, Progress, WindowTarget } from '../bridge/types';

import { acceptPreferences, useAppPreferences } from '../settings/preferences';

const defaults: Preferences = {
  sidebar_width: 240,
  editor_width: 420,
  terminal_height: 240,
  tree_split: 0.6,
  codex_program: null,
  selected_workspace: null,
  sidebar_visible: true,
  editor_visible: false,
  terminal_visible: false,
  workspaces_collapsed: false,
  files_collapsed: false,
  collapsed_nodes: [],
};
export function useApplication() {
  const appearance = useAppPreferences();
  const [ready, setReady] = useState(false);
  const [startupTarget, setStartupTarget] = useState<WindowTarget | null>(null);
  const [error, setError] = useState('');
  const [catalog, setCatalog] = useState<Catalog>({
    servers: [],
    workspaces: [],
    sessions: [],
    live: [],
    open_session_ids: [],
  });
  const [preferences, setPreferences] = useState(defaults);
  const [systemDark, setSystemDark] = useState(matchMedia('(prefers-color-scheme: dark)').matches);
  const [prompts, setPrompts] = useState<{ id: string; prompt: AuthPrompt }[]>([]);
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const closeHandler = useRef<() => void>(() => {});
  const loaded = useRef(false);
  const contentWindow = useRef(false);
  const [refreshing, setRefreshing] = useState(false);
  const pendingRefresh = useRef<Promise<void> | null>(null);
  const invalidated = useRef(false);
  const report = useCallback((error: unknown) => {
    const value = failure(error);
    if (value.code !== 'SESSION_FOCUSED') setError(value.message);
  }, []);
  const refresh = useCallback((invalidate = false): Promise<void> => {
    invalidated.current ||= invalidate;
    if (pendingRefresh.current) return pendingRefresh.current;
    setRefreshing(true);
    const task = (async () => {
      do {
        invalidated.current = false;
        const data = await call('catalog', { archived: false });
        setCatalog(data);
      } while (invalidated.current);
    })().finally(() => {
      pendingRefresh.current = null;
      setRefreshing(false);
    });
    pendingRefresh.current = task;
    return task;
  }, []);
  useEffect(() => {
    const stop = listen((event) => {
      if (event.kind === 'app_preferences_changed') acceptPreferences(event.preferences);
      if (event.kind === 'catalog_changed' && !contentWindow.current) void refresh(true).catch(report);
      if (event.kind === 'operation_finished')
        setProgress((previous) => {
          const next = { ...previous };
          delete next[event.id];
          return next;
        });
      if (event.kind === 'notice') setError(event.message);
      if (event.kind === 'authentication')
        setPrompts((previous) => [...previous, { id: event.id, prompt: event.prompt }]);
      if (event.kind === 'authentication_ended')
        setPrompts((previous) => previous.filter((prompt) => prompt.id !== event.id));
      if (event.kind === 'progress') setProgress((previous) => ({ ...previous, [event.operation]: event.event }));
      if (event.kind === 'close_requested') closeHandler.current();
    });
    void attach()
      .then(async (requestedTarget) => {
        contentWindow.current = requestedTarget?.kind === 'file' || requestedTarget?.kind === 'diff';
        acceptPreferences(await call('app_preferences', { patch: null }));
        const preferences = await call('preferences', { value: null });
        setPreferences(preferences);
        if (!contentWindow.current) await refresh();
        loaded.current = true;
        setStartupTarget(requestedTarget);
        setReady(true);
      })
      .catch(report);
    return stop;
  }, [refresh, report]);
  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const update = () => setSystemDark(media.matches);
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  const dark = appearance.theme === 'dark' || (appearance.theme === 'system' && systemDark);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  }, [dark]);
  useEffect(() => {
    if (!loaded.current) return;
    const timer = setTimeout(() => {
      void call('preferences', { value: preferences }).catch(report);
    }, 350);
    return () => clearTimeout(timer);
  }, [preferences, report]);
  const changePreferences = useCallback(
    (value: Partial<Preferences> | ((previous: Preferences) => Partial<Preferences>)) =>
      setPreferences((previous) => ({ ...previous, ...(typeof value === 'function' ? value(previous) : value) })),
    [],
  );
  const answer = (id: string, answer: string | null) => {
    setPrompts((previous) => previous.filter((prompt) => prompt.id !== id));
    void call('authentication_answer', { id, answer }).catch(report);
  };
  return {
    ready,
    startupTarget,
    error,
    setError,
    report,
    catalog,
    refresh,
    refreshing,
    preferences,
    changePreferences,
    dark,
    prompts,
    answer,
    progress,
    closeHandler,
  };
}
