import { describe, expect, it, vi } from 'vitest';
import type { TextFile } from '../bridge/types';
import { FileTabs, fileKey } from './tabs';

const identity = (context = 'root', root = '/', path = 'workspace/a.txt') => ({
  context,
  root,
  path,
  server: 'dev',
  key: fileKey('dev', root, path),
});
const content = (text: string): TextFile => ({ path: 'workspace/a.txt', text, revision: text });
function pending() {
  let resolve!: (file: TextFile) => void;
  let reject!: (error: Error) => void;
  const task = new Promise<TextFile>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { task, resolve, reject };
}

describe('editor read lifetimes', () => {
  it('coalesces canonical root/workspace reads and reuses unsaved buffers', async () => {
    const request = pending();
    const read = vi.fn(() => request.task);
    const tabs = new FileTabs(read);
    const file = identity();
    const first = tabs.open(file);
    expect(tabs.get(file.key)?.status).toBe('loading');
    expect(tabs.buffers()).toHaveLength(0);
    const second = tabs.open(identity('workspace', '/workspace', 'a.txt'));
    expect(second).toBe(first);
    request.resolve(content('original'));
    await first;
    tabs.update(file.key, { text: 'unsaved' });
    await tabs.open(identity('workspace', '/workspace', 'a.txt'));
    expect(read).toHaveBeenCalledTimes(1);
    expect(tabs.buffers()).toMatchObject([{ text: 'unsaved', original: 'original', context: 'root' }]);
  });

  it('keeps closed reads alive but ignores their results after reopening', async () => {
    const old = pending(),
      latest = pending();
    const read = vi.fn().mockReturnValueOnce(old.task).mockReturnValueOnce(latest.task);
    const tabs = new FileTabs(read);
    const file = identity();
    const first = tabs.open(file);
    tabs.close(file.key);
    expect(tabs.snapshot()).toHaveLength(0);
    expect(tabs.retainedContexts().has('root')).toBe(true);
    const second = tabs.open(identity('workspace', '/workspace', 'a.txt'));
    latest.resolve(content('new'));
    await second;
    old.resolve(content('obsolete'));
    await first;
    expect(tabs.buffers()).toMatchObject([{ text: 'new', context: 'workspace' }]);
    expect([...tabs.retainedContexts()]).toEqual(['workspace']);
    tabs.close(file.key);
    expect(tabs.retainedContexts().size).toBe(0);
  });

  it('retains failed tabs without editable buffers and retries in place', async () => {
    const read = vi.fn().mockRejectedValueOnce(new Error('Connection interrupted')).mockResolvedValue(content('ready'));
    const tabs = new FileTabs(read);
    const file = identity();
    await tabs.open(file);
    await tabs.open(identity('root', '/', 'workspace/b.txt'));
    expect(tabs.get(file.key)).toMatchObject({ status: 'failed', error: 'Connection interrupted' });
    expect(tabs.buffers()).toHaveLength(1);
    expect(tabs.retainedContexts().has('root')).toBe(true);
    await tabs.retry(file.key);
    expect(tabs.snapshot().map((t) => t.path)).toEqual(['workspace/a.txt', 'workspace/b.txt']);
    expect(tabs.get(file.key)?.status).toBe('ready');
  });
});
