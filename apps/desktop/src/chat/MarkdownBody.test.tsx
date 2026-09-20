import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { MarkdownBody } from './MarkdownBody';

const formula = String.raw`\max_I \frac{1}{K}\sum_k L_{R_k}(y\mid I,s)`;
const render = (text: string) => renderToStaticMarkup(<MarkdownBody text={text} report={() => {}} />);

it('preserves remote file hrefs without allowing arbitrary protocols', () => {
  const html = render(
    '[absolute](/home/fixture/project/decision.md)\n\n' +
      '[app](tauri://localhost/home/fixture/project/decision.md)\n\n' +
      '[relative](<notes/中文 report.md>)\n\n' +
      '[script](javascript:alert%281%29)\n\n[foreign](tauri://other/etc/passwd)',
  );
  expect(html).toContain('href="/home/fixture/project/decision.md"');
  expect(html).toContain('href="tauri://localhost/home/fixture/project/decision.md"');
  expect(html).toContain('href="notes/%E4%B8%AD%E6%96%87%20report.md"');
  expect(html).not.toContain('href="javascript:');
  expect(html).not.toContain('href="tauri://other');
});

describe('math in conversation Markdown', () => {
  it.each([
    `[ ${formula} ]`,
    `[\n${formula}\n]`,
    `\\[ ${formula} \\]`,
    `\\[\n${formula}\n\\]`,
    `$$\n${formula}\n$$`,
    `$$ ${formula} $$`,
    '```math\n' + formula + '\n```',
    String.raw`\[\begin{aligned} a &= \frac{1}{2} \\ b &= \sum_{k=1}^K k \end{aligned}\]`,
    String.raw`\[\begin{pmatrix}1 & 2 \\ 3 & 4\end{pmatrix}\]`,
  ])('renders display math: %s', (source) => {
    const html = render(source);
    expect(html).toContain('class="katex-display"');
    expect(html).toContain('<math');
    expect(html).not.toContain('katex-error');
    expect(html).not.toContain('RCMATH');
  });

  it.each([String.raw`The result $x_i^2$ is positive.`, String.raw`The result \(x_i^2 + y_j^2\) is positive.`])(
    'renders inline math without changing the surrounding text',
    (source) => {
      const html = render(source);
      expect(html).toContain('class="katex"');
      expect(html).not.toContain('katex-display');
      expect(html).toContain('The result ');
      expect(html).toContain(' is positive.');
    },
  );

  it('keeps code, links, arrays, ordinary brackets and escaped dollars as Markdown', () => {
    const html = render(
      [
        '`' + `[ ${formula} ]` + '`',
        '```text\n' + String.raw`\[\sum_k x_k\]` + '\n```',
        '[reference](https://example.com)',
        '[a, b, c]',
        '[说明]',
        String.raw`Costs \$5 and \$10.`,
        String.raw`[\(x\)](https://example.com)`,
      ].join('\n\n'),
    );
    expect(html).not.toContain('class="katex');
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain('[a, b, c]');
    expect(html).toContain('Costs $5 and $10.');
  });

  it('preserves Markdown containers without inserting quote markers into TeX', () => {
    for (const source of [`> \\[\n> ${formula}\n> \\]`, `> $$\n> ${formula}\n> $$`, `- [ ${formula} ]`]) {
      const html = render(source);
      expect(html).toContain('katex-display');
      expect(html).not.toContain('katex-error');
      expect(html).not.toContain('<mo>&gt;</mo>');
      expect(html).toContain(source.startsWith('>') ? '<blockquote>' : '<li>');
    }
  });

  it('keeps incomplete or invalid formulas readable and renders completed streamed input', () => {
    for (const incomplete of [String.raw`\[\frac{1}{K}`, String.raw`\(x_i`, '$$\n' + formula]) {
      const html = render(incomplete);
      expect(html).not.toContain('class="katex"');
      expect(html).not.toContain('RCMATH');
    }
    const html = render(`\\[${formula}\\]\n\n$\\notARealCommand{x}$\n\nThe rest is **readable**.`);
    expect(html).toContain('katex-display');
    expect(html).toContain('notARealCommand');
    expect(html).toContain('<strong>readable</strong>');
  });

  it('does not enable raw HTML or trusted TeX commands', () => {
    const html = render(String.raw`<script>alert(1)</script>

\[\href{javascript:alert(1)}{click}\]

\[\includegraphics{https://example.com/private.png}\]`);
    expect(html).not.toContain('<script');
    expect(html).not.toContain('href="javascript:');
    expect(html).not.toContain('<img');
  });
});

it('generates stable, unique heading anchors only for document previews', () => {
  const text = '# One *title*\n\n# One title\n\n# One title-1\n\n# One title\n\n[Jump](#one-title)';
  const preview = () => renderToStaticMarkup(<MarkdownBody text={text} headingIds report={() => {}} />);
  const html = preview();
  expect([...html.matchAll(/<h1 id="([^"]+)"/g)].map((match) => match[1])).toEqual([
    'one-title',
    'one-title-1',
    'one-title-1-1',
    'one-title-2',
  ]);
  expect(html).toContain('href="#one-title"');
  expect(preview()).toBe(html);
  expect(render(text)).not.toContain('id="one-title');
  expect(render(text)).not.toContain('href="#one-title"');
});
