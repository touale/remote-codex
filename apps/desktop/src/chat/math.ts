import type { Nodes, Root } from 'mdast';
import type { Parser, Plugin } from 'unified';

type Formula = { start: number; end: number; value: string; display: boolean };
const literal = new Set([
  'code',
  'inlineCode',
  'link',
  'linkReference',
  'image',
  'imageReference',
  'definition',
  'html',
  'math',
  'inlineMath',
]);

function walk(node: Nodes, visit: (node: Nodes) => boolean | void) {
  if (visit(node) !== false && 'children' in node) {
    for (const child of node.children) walk(child, visit);
  }
}

function escaped(source: string, offset: number): boolean {
  let count = 0;
  while (offset > 0 && source[--offset] === '\\') count++;
  return count % 2 === 1;
}

function expression(source: string, start: number, end: number): string {
  const prefix = source.slice(source.lastIndexOf('\n', start - 1) + 1, start);
  const quotes = prefix.match(/^(?:\s*>\s*)+/)?.[0].match(/>/g)?.length ?? 0;
  return source
    .slice(start, end)
    .split('\n')
    .map((line, index) => {
      // Source offsets include Markdown quote markers on continuation lines.
      for (let depth = 0; index > 0 && depth < quotes; depth++) line = line.replace(/^[ \t]*> ?/, '');
      return line;
    })
    .join('\n');
}

// Use Markdown's own source ranges to protect code and links. Reparse only when
// alternate delimiters exist; placeholders keep TeX underscores and brackets
// out of Markdown, and the original expression is restored before rendering.
export const remarkMathDelimiters: Plugin<[], Root> = function () {
  const parse = this.parser! as Parser<Root>;
  this.parser = (source, file) => {
    let tree = parse(source, file);
    const formulas: Formula[] = [];
    const protectedRanges: { start: number; end: number }[] = [];
    if (/\\[([a-zA-Z]/.test(source)) {
      walk(tree, (node) => {
        const start = node.position?.start.offset;
        const end = node.position?.end.offset;
        if (start === undefined || end === undefined) return;
        if (literal.has(node.type)) {
          protectedRanges.push({ start, end });
          return false;
        }
        if (node.type === 'paragraph') {
          const raw = expression(source, start, end);
          if (
            raw.startsWith('[') &&
            raw.endsWith(']') &&
            /\\[a-zA-Z]+/.test(raw) &&
            node.children.every((child) => !literal.has(child.type))
          ) {
            formulas.push({ start, end, value: raw.slice(1, -1).trim(), display: true });
          }
        }
      });
      const openings = /\\([([])/g;
      let opening: RegExpExecArray | null;
      while ((opening = openings.exec(source))) {
        const start = opening.index;
        if (escaped(source, start) || formulas.some((part) => start >= part.start && start < part.end)) continue;
        const close = opening[1] === '[' ? '\\]' : '\\)';
        let end = source.indexOf(close, start + 2);
        while (end >= 0 && escaped(source, end)) end = source.indexOf(close, end + 2);
        if (end < 0 || protectedRanges.some((part) => start < part.end && end + 2 > part.start)) continue;
        formulas.push({
          start,
          end: end + 2,
          value: expression(source, start, end).slice(2).trim(),
          display: opening[1] === '[',
        });
        openings.lastIndex = end + 2;
      }
    }
    const restored = new Map<number, Formula>();
    if (formulas.length) {
      let normalized = '';
      let cursor = 0;
      for (const formula of formulas.sort((a, b) => a.start - b.start)) {
        if (
          formula.start < cursor ||
          protectedRanges.some((part) => formula.start < part.end && formula.end > part.start)
        )
          continue;
        normalized += source.slice(cursor, formula.start);
        restored.set(normalized.length, formula);
        normalized += '$$RCMATH$$';
        cursor = formula.end;
      }
      normalized += source.slice(cursor);
      tree = parse(normalized, file);
      source = normalized;
    }
    walk(tree, (node) => {
      if (node.type !== 'inlineMath' && node.type !== 'math') return;
      const start = node.position?.start.offset;
      const end = node.position?.end.offset;
      if (start === undefined || end === undefined) return;
      const formula = restored.get(start);
      const raw = expression(source, start, end);
      // remark-math accepts an unfinished block at EOF. Keep it readable while
      // streaming instead of presenting a formula before its closing delimiter.
      const lines = raw.split('\n');
      const openingLength = raw.match(/^\${2,}/)?.[0].length ?? 2;
      const closing = lines.at(-1)?.trim() ?? '';
      if (node.type === 'math' && (lines.length < 2 || !/^\$+$/.test(closing) || closing.length < openingLength)) {
        node.data = { hName: 'code', hProperties: {}, hChildren: [{ type: 'text', value: raw }] };
        return;
      }
      if (!formula) return;
      node.value = formula.value;
      node.data = {
        hName: 'code',
        hProperties: { className: ['language-math', formula.display ? 'math-display' : 'math-inline'] },
        hChildren: [{ type: 'text', value: formula.value }],
      };
    });
    // A standalone $$...$$ paragraph is display math even on a single line.
    walk(tree, (node) => {
      if (node.type !== 'paragraph' || node.children.length !== 1) return;
      const child = node.children[0];
      if (child.type !== 'inlineMath') return;
      const start = child.position?.start.offset;
      if (start !== undefined && !restored.has(start) && source.slice(start).startsWith('$$')) {
        child.data!.hProperties = { className: ['language-math', 'math-display'] };
      }
    });
    return tree;
  };
};
