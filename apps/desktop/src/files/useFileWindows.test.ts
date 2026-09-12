import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import { call } from '../bridge/client';
import { FileTabs, fileKey } from './tabs';
import { useFileWindows } from './useFileWindows';
vi.mock('../bridge/client', () => ({
  call: vi.fn(),
  failure: (e: unknown) => e,
  operationId: () => crypto.randomUUID(),
}));

it('keeps and unlocks the source on failure, and removes it only after the destination accepts the dirty document', async () => {
  const file = {
    server: 'dev',
    root: '/workspace',
    path: 'a.txt',
    text: 'unsaved',
    original: 'saved',
    revision: 'revision',
  };
  const source = new FileTabs(vi.fn());
  const key = source.adopt('source-context', file);
  const other = source.adopt('source-context', { ...file, path: 'other.txt' });
  let actions!: ReturnType<typeof useFileWindows>;
  function Harness() {
    actions = useFileWindows(source);
    return null;
  }
  renderToString(createElement(Harness));
  let reject!: (error: unknown) => void;
  vi.mocked(call).mockImplementationOnce(
    () =>
      new Promise((_, fail) => {
        reject = fail;
      }),
  );
  const failed = actions.moveWindow(key);
  expect(source.buffers()[0].transferring).toBeTruthy();
  source.update(key, { text: 'late editor input' });
  expect(source.buffers()[0].text).toBe('unsaved');
  reject({ code: 'WINDOW_ERROR', message: 'Failed to open' });
  await expect(failed).rejects.toMatchObject({ code: 'WINDOW_ERROR' });
  expect(source.buffers()[0]).toMatchObject({ text: 'unsaved', transferring: undefined });
  let accept!: (label: string) => void;
  vi.mocked(call).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        accept = resolve;
      }),
  );
  const moved = actions.moveWindow(key);
  expect(source.buffers()).toHaveLength(2);
  const target = vi.mocked(call).mock.calls.at(-1)![1] as { target: { document: typeof file } };
  const destination = new FileTabs(vi.fn());
  destination.adopt('destination-context', target.target.document);
  accept('content-window');
  await moved;
  expect(source.snapshot().map((tab) => tab.key)).toEqual([other]);
  expect(destination.get(fileKey('dev', '/workspace', 'a.txt'))).toMatchObject({
    ...file,
    context: 'destination-context',
  });
});
