import path from 'node:path';
import fs from 'node:fs';
import { browser } from '@wdio/globals';

const artifacts = path.resolve('../../.artifacts/desktop-e2e');
fs.mkdirSync(artifacts, { recursive: true });
const root = process.env.REMOTE_CODEX_E2E_ROOT ?? fs.mkdtempSync('/tmp/rc-desktop-ui-');
process.env.REMOTE_CODEX_DATA_DIR = path.join(root, 'state');
process.env.REMOTE_CODEX_DESKTOP_STATE_DIR = path.join(root, 'preferences');
process.env.CODEX_HOME = path.join(root, 'codex');
export const config = {
  runner: 'local',
  framework: 'mocha',
  reporters: ['spec'],
  maxInstances: 1,
  specs: process.env.REMOTE_CODEX_PERFORMANCE
    ? ['./tests/performance.e2e.ts']
    : [
        './tests/desktop.e2e.ts',
        './tests/controls.e2e.ts',
        ...(process.env.REMOTE_CODEX_E2E_TARGET ? ['./tests/workspace.e2e.ts'] : []),
      ],
  logLevel: 'warn',
  outputDir: artifacts,
  services: [
    [
      '@wdio/tauri-service',
      { appBinaryPath: path.resolve('../../target/debug/remote-codex-desktop'), driverProvider: 'embedded' },
    ],
  ],
  capabilities: [{ browserName: 'tauri' }],
  waitforTimeout: 15000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 0,
  mochaOpts: { timeout: 45000, bail: true },
  afterTest: async (test: { title: string }, _context: unknown, result: { passed: boolean }) => {
    if (result.passed) return;
    const name = test.title.replace(/[^a-z0-9]+/gi, '-').slice(0, 80);
    await browser.saveScreenshot(path.join(artifacts, `failure-${name}.png`));
  },
};
