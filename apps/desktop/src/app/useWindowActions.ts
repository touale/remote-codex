import { useEffect } from 'react';
import { call, listen } from '../bridge/client';
import { visibleIn } from '../files/context';
import type { useFileActions } from '../files/useFileActions';
import type { useFiles } from '../files/useFiles';
import type { useApplication } from './useApplication';
import type { useNavigation } from './useNavigation';
export function useWindowActions(
  app: Pick<ReturnType<typeof useApplication>, 'preferences' | 'report' | 'changePreferences'>,
  nav: Pick<ReturnType<typeof useNavigation>, 'fileContext' | 'focusSession'>,
  actions: Pick<ReturnType<typeof useFileActions>, 'save'>,
  files: Pick<ReturnType<typeof useFiles>, 'tabs' | 'selected'>,
  newSession: () => void,
  toggleTerminal: () => void,
  openSettings: () => void,
) {
  const prefs = app.preferences;
  const run = (promise: Promise<unknown>) => void promise.catch(app.report);
  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (!event.metaKey && !event.ctrlKey) return;
      if (event.key.toLowerCase() === 'b') {
        event.preventDefault();
        app.changePreferences({ sidebar_visible: !prefs.sidebar_visible });
      }
      if (event.key.toLowerCase() === 's' && nav.fileContext && !document.querySelector('[role=dialog]')) {
        event.preventDefault();
        event.stopPropagation();
        const visible = files.tabs.filter((b) => visibleIn(nav.fileContext, b));
        const key = (visible.find((b) => b.key === files.selected[nav.fileContext!.id]) ?? visible.at(-1))?.key;
        if (key) run(actions.save(key));
      }
    };
    document.addEventListener('keydown', keyboard, true);
    const unlisten = listen((event) => {
      if (event.kind === 'focus_session') nav.focusSession(event.id);
      if (event.kind === 'ui_action') {
        if (event.action !== 'new_window' && document.querySelector('[role=dialog]')) return;
        if (event.action === 'new_window') run(call('new_window', { target: null }));
        if (event.action === 'new_session') newSession();
        if (event.action === 'settings') openSettings();
        if (event.action === 'terminal') toggleTerminal();
      }
    });
    return () => {
      document.removeEventListener('keydown', keyboard, true);
      unlisten();
    };
  });
}
