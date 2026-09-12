import { parsePatch } from 'diff';

interface PatchLine {
  number: number;
  text: string;
}
interface PatchRow {
  before?: PatchLine;
  after?: PatchLine;
  changed: boolean;
}
interface PatchHunk {
  label: string;
  omitted: boolean;
  noNewline: boolean;
  unified: PatchRow[];
  split: PatchRow[];
}

export function parseChanges(text: string): PatchHunk[] {
  return parsePatch(text).flatMap((file) =>
    file.hunks.map((hunk, index) => {
      let before = hunk.oldStart;
      let after = hunk.newStart;
      const unified: PatchRow[] = [];
      const split: PatchRow[] = [];
      let removed: PatchLine[] = [];
      let added: PatchLine[] = [];
      const flush = () => {
        for (let i = 0; i < Math.max(removed.length, added.length); i++)
          split.push({ before: removed[i], after: added[i], changed: true });
        removed = [];
        added = [];
      };
      for (const line of hunk.lines) {
        const text = line.slice(1);
        if (line.startsWith('-')) {
          const entry = { number: before++, text };
          removed.push(entry);
          unified.push({ before: entry, changed: true });
        } else if (line.startsWith('+')) {
          const entry = { number: after++, text };
          added.push(entry);
          unified.push({ after: entry, changed: true });
        } else if (line.startsWith(' ')) {
          flush();
          const row = { before: { number: before++, text }, after: { number: after++, text }, changed: false };
          unified.push(row);
          split.push(row);
        } else if (!line.startsWith('\\')) throw new Error('Unsupported patch line');
      }
      flush();
      const previous = file.hunks[index - 1];
      const omitted =
        hunk.oldStart > (previous ? previous.oldStart + previous.oldLines : 1) ||
        hunk.newStart > (previous ? previous.newStart + previous.newLines : 1);
      return {
        label: `@@ -${hunk.oldStart},${hunk.oldLines} +${hunk.newStart},${hunk.newLines} @@`,
        omitted,
        noNewline: hunk.lines.some((line) => line.startsWith('\\ No newline')),
        unified,
        split,
      };
    }),
  );
}
