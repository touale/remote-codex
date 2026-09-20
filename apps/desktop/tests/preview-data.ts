// Small synthetic assets. No developer files or remote credentials are used.
export function previewPdf() {
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] /Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>',
    '',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>',
    '',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  for (const [index, label] of [
    [3, 'Preview page one'],
    [5, 'Preview page two'],
  ] as const) {
    const content = `BT /F1 22 Tf 40 430 Td (${label}) Tj ET`;
    objects[index] = `<< /Length ${content.length} >>\nstream\n${content}\nendstream`;
  }
  let pdf = '%PDF-1.4\n';
  const offsets = [0];
  objects.forEach((object, index) => {
    offsets.push(pdf.length);
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xref = pdf.length;
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  pdf += offsets
    .slice(1)
    .map((offset) => `${String(offset).padStart(10, '0')} 00000 n \n`)
    .join('');
  pdf += `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
  return new TextEncoder().encode(pdf);
}
export const previewSvg = new TextEncoder().encode(
  '<svg xmlns="http://www.w3.org/2000/svg" width="360" height="220"><rect x="12" y="12" width="336" height="196" rx="16" fill="#e7eef7"/><circle cx="92" cy="110" r="46" fill="#4b719e"/><text x="158" y="117" font-family="sans-serif" font-size="22">Preview</text></svg>',
);
export const previewMarkdown =
  '# Preview document\n\n未保存的内容也可以预览。\n\n$$E=mc^2$$\n\n| Feature | Status |\n| --- | --- |\n| Preview | Ready |\n\n![Diagram](./diagram.svg)\n\n[Next](./next.md) · [Outside](../../private.md)\n';
