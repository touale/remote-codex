export type Axis = 'x' | 'y';
export interface Surface {
  element: HTMLElement;
  x: boolean;
  y: boolean;
}

export function nativeSurface(element: HTMLElement): Surface | null {
  const x = element.scrollWidth > element.clientWidth + 1;
  const y = element.scrollHeight > element.clientHeight + 1;
  if (!x && !y) return null;
  const style = getComputedStyle(element);
  const surface = {
    element,
    x: x && /^(auto|scroll)$/.test(style.overflowX),
    y: y && /^(auto|scroll)$/.test(style.overflowY),
  };
  return surface.x || surface.y ? surface : null;
}

export function edgeAxes(surface: Surface, x: number, y: number): Axis[] {
  const rect = surface.element.getBoundingClientRect();
  if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return [];
  const axes: Axis[] = [];
  if (surface.x && y >= rect.bottom - 16) axes.push('x');
  if (surface.y && x >= rect.right - 16) axes.push('y');
  return axes;
}
