import type { Nodes, Root } from 'mdast';
import type { Plugin } from 'unified';

function text(node: Nodes): string {
  if ('value' in node) return node.value;
  if ('alt' in node) return node.alt ?? '';
  return 'children' in node ? node.children.map(text).join('') : '';
}

// Generate IDs once per parsed document, independent of React render order.
export const remarkHeadingIds: Plugin<[], Root> = () => (tree) => {
  const counts = new Map<string, number>();
  const visit = (node: Nodes) => {
    if (node.type === 'heading') {
      const slug = text(node)
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s_-]/gu, '')
        .replace(/\s/g, '-');
      let id = slug;
      while (counts.has(id)) {
        const next = (counts.get(slug) ?? 0) + 1;
        counts.set(slug, next);
        id = `${slug}-${next}`;
      }
      counts.set(id, 0);
      node.data = { ...node.data, hProperties: { ...node.data?.hProperties, id } };
    }
    if ('children' in node) node.children.forEach(visit);
  };
  visit(tree);
};
