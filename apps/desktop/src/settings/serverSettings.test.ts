import { expect, test } from 'vitest';
import type { ConfigReport } from '../bridge/types';
import { serverSettingsChanges } from './serverSettings';

const current = { useProxy: true, proxy: 'http://https-proxy:7890', background: true, retryAttempts: '10' };
const saved: ConfigReport = {
  saved_revision: 4,
  applied_revision: 4,
  items: Object.entries({
    'proxy.mode': 'custom',
    'env.https_proxy': current.proxy,
    'env.http_proxy': 'http://http-proxy:8080',
    background: true,
    'reconnect.max_attempts': 10,
  }).map(([key, value]) => ({ key, value, application_state: 'applied' })),
};

test('editing retries leaves distinct proxy addresses and other settings untouched', () => {
  expect(serverSettingsChanges({ ...current, retryAttempts: '20' }, saved)).toEqual([['reconnect.max_attempts', '20']]);
  expect(serverSettingsChanges(current, saved)).toEqual([]);
});

test('explicit proxy changes still update the proxy settings', () => {
  expect(serverSettingsChanges({ ...current, useProxy: false }, saved)).toEqual([['proxy.mode', 'direct']]);
  expect(serverSettingsChanges({ ...current, proxy: 'http://new-proxy:7890' }, saved)).toEqual([
    ['proxy.mode', 'custom'],
    ['env.https_proxy', 'http://new-proxy:7890'],
    ['env.http_proxy', 'http://new-proxy:7890'],
  ]);
});
