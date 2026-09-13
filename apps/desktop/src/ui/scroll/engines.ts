import type { Surface } from './surfaces';

/** Pinned renderer DOM adapters; neither adapter owns scrolling or input. */
export function engineSurface(element: HTMLElement): Surface | null | undefined {
  if (element.classList.contains('xterm')) {
    // Terminal screen gestures must resolve to the renderer-owned scrollbar.
    const viewport = element.querySelector<HTMLElement>('.xterm-scrollable-element');
    if (!viewport) return null;
    element = viewport;
  }
  if (element.matches('.monaco-scrollable-element, .xterm-scrollable-element')) {
    // Both renderers virtualize content; thumb range determines overflow.
    const overflows = (direction: string, dimension: 'clientWidth' | 'clientHeight') => {
      const bar = element.querySelector<HTMLElement>(`:scope > .scrollbar.${direction}`);
      const slider = bar?.querySelector<HTMLElement>('.slider');
      return Boolean(bar && slider && slider[dimension] > 0 && slider[dimension] < bar[dimension]);
    };
    const x = overflows('horizontal', 'clientWidth');
    const y = overflows('vertical', 'clientHeight');
    return x || y ? { element, x, y } : null;
  }
  return undefined;
}
