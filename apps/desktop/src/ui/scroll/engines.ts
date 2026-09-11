import { nativeSurface, type Surface } from './surfaces';

/** Pinned renderer DOM adapters; neither adapter owns scrolling or input. */
export function engineSurface(element: HTMLElement): Surface | null | undefined {
  if (element.classList.contains('xterm')) {
    // The viewport is a sibling of the screen hit by pointer events in xterm 5.
    const viewport = element.querySelector<HTMLElement>('.xterm-viewport');
    return viewport ? nativeSurface(viewport) : null;
  }
  if (element.classList.contains('monaco-scrollable-element')) {
    // Monaco virtualizes content. Its rendered thumb range is the source of overflow.
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
