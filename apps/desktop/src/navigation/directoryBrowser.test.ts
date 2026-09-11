import { describe, expect, it, vi } from 'vitest';
import { DirectoryBrowser } from './directoryBrowser';
const page = { entries: [], truncated: false };

describe('directory browser ownership', () => {
  it('keeps its lease after a read error and releases it once after creation', async () => {
    const invoke = vi.fn(async (command: string, args: Record<string, unknown>) => {
      if (command === 'directory_open') return 'browser';
      if (command === 'directory_browse' && args.path === '/missing') throw { message: 'Missing directory' };
      if (command === 'directory_create') return '/project/new';
      return page;
    });
    const browser = new DirectoryBrowser('dev', vi.fn(), invoke as never);
    await expect(browser.list('/missing')).rejects.toMatchObject({ message: 'Missing directory' });
    await browser.list('/project');
    await browser.create('/project', 'new');
    await browser.list('/project/new');
    browser.dispose();
    browser.dispose();
    expect(invoke.mock.calls.filter(([command]) => command === 'directory_open')).toHaveLength(1);
    expect(invoke.mock.calls.filter(([command]) => command === 'directory_close')).toHaveLength(1);
    await expect(browser.list('/')).rejects.toMatchObject({ code: 'OPERATION_CANCELLED' });
  });
  it('cancels pending opens and releases a late handle without sending a directory read', async () => {
    let finish!: (id: string) => void;
    const invoke = vi.fn((command: string) =>
      command === 'directory_open'
        ? new Promise<string>((resolve) => {
            finish = resolve;
          })
        : Promise.resolve(),
    );
    const browser = new DirectoryBrowser('dev', vi.fn(), invoke as never);
    const pending = browser.list('/');
    browser.dispose();
    finish('late-handle');
    await expect(pending).rejects.toMatchObject({ code: 'OPERATION_CANCELLED' });
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      'directory_open',
      'cancel_operation',
      'directory_close',
    ]);
  });
});
