import { expect, it } from 'vitest';
import { parseChanges } from './patch';

it('keeps real line numbers and aligns unequal replacements without joining separate hunks', () => {
  const hunks = parseChanges(
    '--- a/a.txt\n+++ b/a.txt\n@@ -10,3 +10,4 @@\n context\n-old\n+new\n+extra\n tail\n@@ -90 +91 @@\n-later\n+changed\n',
  );
  expect(hunks).toHaveLength(2);
  expect(hunks[0].split).toEqual([
    { before: { number: 10, text: 'context' }, after: { number: 10, text: 'context' }, changed: false },
    { before: { number: 11, text: 'old' }, after: { number: 11, text: 'new' }, changed: true },
    { before: undefined, after: { number: 12, text: 'extra' }, changed: true },
    { before: { number: 12, text: 'tail' }, after: { number: 13, text: 'tail' }, changed: false },
  ]);
  expect(hunks[1].split[0]).toMatchObject({ before: { number: 90 }, after: { number: 91 } });
  expect(hunks[0].unified).toHaveLength(5);
});
it('handles headerless additions, deletions and empty lines without counting EOF markers as content', () => {
  expect(parseChanges('@@ -0,0 +1,2 @@\n+hello\n+\n\\ No newline at end of file\n')[0].split).toEqual([
    { before: undefined, after: { number: 1, text: 'hello' }, changed: true },
    { before: undefined, after: { number: 2, text: '' }, changed: true },
  ]);
  expect(parseChanges('@@ -1 +0,0 @@\n-gone\n')[0].split[0]).toEqual({
    before: { number: 1, text: 'gone' },
    after: undefined,
    changed: true,
  });
  expect(parseChanges('')).toEqual([]);
  expect(parseChanges('unrecognized content')).toEqual([]);
});
