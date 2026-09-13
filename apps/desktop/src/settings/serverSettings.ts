import type { ConfigReport } from '../bridge/types';

interface ServerSettings {
  useProxy: boolean;
  proxy: string;
  background: boolean;
  retryAttempts: string;
}

export function serverSettingsChanges(current: ServerSettings, saved: ConfigReport | null): [string, string][] {
  const value = (key: string) => saved?.items.find((item) => item.key === key)?.value;
  const changes: [string, string][] = [];
  if (
    !saved ||
    current.useProxy !== (value('proxy.mode') === 'custom') ||
    (current.useProxy && current.proxy !== String(value('env.https_proxy') ?? ''))
  ) {
    changes.push(['proxy.mode', current.useProxy ? 'custom' : 'direct']);
    if (current.useProxy) changes.push(['env.https_proxy', current.proxy], ['env.http_proxy', current.proxy]);
  }
  if (!saved || current.background !== (value('background') !== false)) {
    changes.push(['background', String(current.background)]);
  }
  if (!saved || current.retryAttempts !== String(value('reconnect.max_attempts') ?? 10)) {
    changes.push(['reconnect.max_attempts', current.retryAttempts]);
  }
  return changes;
}
