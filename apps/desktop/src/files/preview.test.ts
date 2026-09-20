import { expect, it } from 'vitest';
import { FileTabs, fileKey } from './tabs';
import { isMarkdown, previewType } from './preview';
import { workspaceFilePath } from './links';

it('recognizes preview formats and resolves document resources within their file context', () => {
  expect(previewType('paper.PDF')?.format).toBe('pdf');
  expect(previewType('plot.JPEG')?.mime).toBe('image/jpeg');
  for (const path of ['source.ts', 'file.constructor', 'file.__proto__', 'pdf', 'svg', 'dir.pdf/plain']) {
    expect(previewType(path)).toBeNull();
  }
  expect(isMarkdown('REPORT.md')).toBe(true);
  expect(workspaceFilePath('../images/figure.svg', '/project', '/project/notes')).toBe('images/figure.svg');
  expect(() => workspaceFilePath('../../private.png', '/project', '/project/notes')).toThrow('outside');
});

it('keeps previews out of text buffers, cancels closed reads and rejects late results', async () => {
  let finish!: (value: { preview: { format: 'pdf'; mime: string; data: ArrayBuffer } }) => void;
  let signal!: AbortSignal;
  const content = { preview: { format: 'pdf' as const, mime: 'application/pdf', data: new ArrayBuffer(10) } };
  const tabs = new FileTabs(async (_, __, cancel) => {
    signal = cancel;
    return new Promise((resolve) => {
      finish = resolve;
    });
  });
  const file = { key: fileKey('dev', '/', 'paper.pdf'), context: 'files', server: 'dev', root: '/', path: 'paper.pdf' };
  const read = tabs.open(file);
  await Promise.resolve();
  tabs.close(file.key);
  expect(signal.aborted).toBe(true);
  finish(content);
  await read;
  expect(tabs.snapshot()).toEqual([]);
  const next = tabs.open(file);
  await Promise.resolve();
  finish(content);
  await next;
  expect(tabs.buffers()).toEqual([]);
  expect(tabs.ready(file.key)).toMatchObject(content);
  tabs.update(file.key, { text: 'must not become editable' });
  expect(tabs.get(file.key)).not.toHaveProperty('text');
  tabs.relocate('dev', '/paper.pdf', '/renamed.pdf');
  expect(tabs.snapshot()[0]).toMatchObject({ path: 'renamed.pdf', ...content });
});
