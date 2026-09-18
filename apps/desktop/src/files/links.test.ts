import { expect, it } from 'vitest';
import { linkKind, workspaceFilePath } from './links';

const workspace = '/home/fixture/project';

it('resolves conversation file links against the remote workspace', () => {
  for (const href of [
    '/home/fixture/project/decision.md',
    'tauri://localhost/home/fixture/project/decision.md',
    './decision.md',
    'sub/../decision.md',
    'decision.md#section',
  ]) {
    expect(linkKind(href)).toBe('file');
    expect(workspaceFilePath(href, workspace)).toBe('decision.md');
  }
  expect(workspaceFilePath('notes/%E4%B8%AD%E6%96%87%20report.md', workspace)).toBe('notes/中文 report.md');
  expect(workspaceFilePath('notes/100%25%23result.md', workspace)).toBe('notes/100%#result.md');
  expect(workspaceFilePath('%252e%252e/report.md', workspace)).toBe('%2e%2e/report.md');
  expect(workspaceFilePath('/notes.md', '/')).toBe('notes.md');
});

it('rejects escaped roots, malformed paths and non-file protocols before reading files', () => {
  for (const href of [
    '../private.md',
    '%2e%2e/private.md',
    '%2Fetc/passwd',
    '/home/fixture/project-other/private.md',
    'tauri://localhost/etc/passwd',
    '.',
    workspace,
    '%ZZ',
    'file%00.md',
    'file%5Cname.md',
    'file:///etc/passwd',
    'javascript:alert(1)',
    'data:text/html,secret',
    'tauri://other/home/fixture/project/report.md',
    'tauri://user@localhost/home/fixture/project/report.md',
    '//example.com/report.md',
    '#section',
  ])
    expect(() => workspaceFilePath(href, workspace), href).toThrow();
  expect(linkKind('https://example.com/report.md')).toBe('web');
  expect(linkKind('http://localhost/report.md')).toBe('web');
});
