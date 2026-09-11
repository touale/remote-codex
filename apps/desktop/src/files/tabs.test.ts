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
  it('keeps colliding drafts independent and retries a move after the target closes', async () => {
    const tabs = new FileTabs(async (_, path) => ({ path, text: path, revision: path }));
    const source = identity('root', '/', 'workspace/source.txt');
    const target = identity('root', '/', 'workspace/target.txt');
    await tabs.open(source);
    await tabs.open(target);
    tabs.update(source.key, { text: 'Source edits' });
    tabs.update(target.key, { text: 'Target edits' });
    expect(() => tabs.assertCanMove('dev', '/workspace/source.txt', '/workspace/target.txt')).toThrow(
      'destination tab',
    );
    const keys = tabs.relocate('dev', '/workspace/source.txt', '/workspace/target.txt');
    expect(keys.size).toBe(0);
    expect(new Set(tabs.snapshot().map((tab) => tab.key)).size).toBe(2);
    expect(() => tabs.assertWritable(source.key)).toThrow('conflicting destination tab');
    tabs.update(source.key, { text: 'More source edits' });
    expect(tabs.get(target.key)).toMatchObject({ text: 'Target edits' });
    tabs.relocate('dev', '/workspace/target.txt', '/archive/target.txt');
    expect(tabs.get(source.key)).toMatchObject({ pendingMove: '/archive/target.txt', path: source.path });
    tabs.close(fileKey('dev', '/', 'archive/target.txt'));
    const resumed = tabs.relocate('dev', '/archive/target.txt', '/archive/target.txt').get(source.key)!;
    expect(tabs.get(resumed)).toMatchObject({
      text: 'More source edits',
      path: 'archive/target.txt',
      pendingMove: undefined,
    });
    expect(() => tabs.assertWritable(resumed)).not.toThrow();
    expect(tabs.snapshot()).toHaveLength(1);
  });

  it('replaces a clean destination without accepting its stale read or conflict path', async () => {
    const late = pending();
    const source = identity('root', '/', 'workspace/source.txt');
    const target = identity('root', '/', 'workspace/target.txt');
    const tabs = new FileTabs((_, path) => (path === source.path ? Promise.resolve(content('source')) : late.task));
    await tabs.open(source);
    tabs.update(source.key, {
      text: 'Unsaved source',
      conflict: { path: source.path, text: 'Remote source', revision: 'remote' },
    });
    const read = tabs.open(target);
    const moved = tabs.relocate('dev', '/workspace/source.txt', '/workspace/target.txt').get(source.key)!;
    late.resolve(content('obsolete destination'));
    await read;
    expect(tabs.snapshot()).toHaveLength(1);
    expect(tabs.get(moved)).toMatchObject({
      text: 'Unsaved source',
      conflict: { path: target.path, text: 'Remote source' },
    });
  });

  it('relocates directory buffers, preserves edits, and rejects pending reads from the old path', async () => {
    const old = pending();
    const read = vi
      .fn()
      .mockResolvedValueOnce(content('original'))
      .mockReturnValueOnce(old.task)
      .mockResolvedValue(content('moved'));
    const tabs = new FileTabs(read);
    const dirty = identity('narrow', '/workspace', 'a.txt');
    await tabs.open(dirty);
    tabs.update(dirty.key, { text: 'unsaved' });
    const loading = identity('root', '/', 'workspace/b.txt');
    const request = tabs.open(loading);
    await Promise.resolve();
    const keys = tabs.relocate('dev', '/workspace', '/archive/workspace');
    const key = keys.get(dirty.key)!;
    expect(tabs.get(key)).toMatchObject({
      root: '/',
      path: 'archive/workspace/a.txt',
      context: '',
      text: 'unsaved',
      original: 'original',
      revision: 'original',
    });
    tabs.bind(key, 'owned-root');
    await tabs.retry(keys.get(loading.key)!);
    old.resolve(content('obsolete'));
    await request;
    expect(tabs.get(keys.get(loading.key)!)!).toMatchObject({ text: 'moved', path: 'archive/workspace/b.txt' });
    expect(tabs.get(dirty.key)).toBeUndefined();
    expect(tabs.retainedContexts().has('narrow')).toBe(false);
    expect(tabs.get(key)).toMatchObject({ context: 'owned-root', text: 'unsaved' });
  });

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
