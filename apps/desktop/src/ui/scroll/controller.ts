import { engineSurface } from './engines';
import { edgeAxes, nativeSurface, type Axis } from './surfaces';

interface Active {
  element: HTMLElement;
  axis: Axis;
  timer?: ReturnType<typeof setTimeout>;
}

/** One controller per WebView, including portals and dynamically rendered content. */
export function installScrollbars(document: Document) {
  const window = document.defaultView!;
  const active = new Set<Active>();
  let dragging = false;
  let frame = 0;
  let fallback: ReturnType<typeof setTimeout> | undefined;
  let pointer: { path: EventTarget[]; x: number; y: number } | undefined;
  const hide = (item: Active) => {
    clearTimeout(item.timer);
    item.element.removeAttribute(`data-scroll-${item.axis}`);
    active.delete(item);
  };
  const reset = () => {
    window.cancelAnimationFrame(frame);
    clearTimeout(fallback);
    frame = 0;
    pointer = undefined;
    dragging = false;
    for (const item of active) hide(item);
  };
  const update = () => {
    window.cancelAnimationFrame(frame);
    clearTimeout(fallback);
    frame = 0;
    if (dragging) return;
    let element: HTMLElement | undefined;
    let axes: Axis[] = [];
    if (pointer)
      for (const target of pointer.path) {
        if (!(target instanceof HTMLElement) || !target.isConnected) continue;
        const engine = engineSurface(target);
        const surface = engine === undefined ? nativeSurface(target) : engine;
        if (!surface) continue;
        element = surface.element;
        axes = edgeAxes(surface, pointer.x, pointer.y);
        // The nearest scroll surface owns the gesture, even away from its edge.
        break;
      }
    for (const item of active) {
      if (!item.element.isConnected) hide(item);
      else if (item.element === element && axes.includes(item.axis)) {
        clearTimeout(item.timer);
        item.timer = undefined;
      } else if (!item.timer) item.timer = setTimeout(() => hide(item), 800);
    }
    if (element)
      for (const axis of axes) {
        if ([...active].some((item) => item.element === element && item.axis === axis)) continue;
        element.setAttribute(`data-scroll-${axis}`, '');
        active.add({ element, axis });
      }
  };
  const remember = (event: PointerEvent) => {
    pointer = { path: event.composedPath(), x: event.clientX, y: event.clientY };
  };
  const schedule = () => {
    if (frame) return;
    frame = window.requestAnimationFrame(update);
    // WebKit can suspend RAF in an unfocused window receiving pointer events.
    fallback = setTimeout(update, 50);
  };
  const move = (event: PointerEvent) => {
    if (event.pointerType === 'touch') return;
    remember(event);
    schedule();
  };
  const down = (event: PointerEvent) => {
    if (event.button !== 0 || event.pointerType === 'touch') return;
    remember(event);
    window.cancelAnimationFrame(frame);
    update();
    dragging = [...active].some((item) => item.timer === undefined);
  };
  const up = (event: PointerEvent) => {
    dragging = false;
    remember(event);
    update();
  };
  const leave = (event: PointerEvent) => {
    if (event.relatedTarget !== null) return;
    pointer = undefined;
    update();
  };
  const resize = () => {
    schedule();
  };
  document.addEventListener('pointermove', move, { capture: true, passive: true });
  document.addEventListener('pointerdown', down, { capture: true, passive: true });
  document.addEventListener('pointerout', leave, { capture: true, passive: true });
  window.addEventListener('pointerup', up, { capture: true, passive: true });
  window.addEventListener('pointercancel', reset);
  window.addEventListener('blur', reset);
  window.addEventListener('resize', resize);
  document.addEventListener('visibilitychange', reset);
  return () => {
    reset();
    document.removeEventListener('pointermove', move, true);
    document.removeEventListener('pointerdown', down, true);
    document.removeEventListener('pointerout', leave, true);
    window.removeEventListener('pointerup', up, true);
    window.removeEventListener('pointercancel', reset);
    window.removeEventListener('blur', reset);
    window.removeEventListener('resize', resize);
    document.removeEventListener('visibilitychange', reset);
  };
}
