import { useEffect, useRef } from 'react';
export function ResizeHandle({
  axis,
  value,
  min,
  max,
  onChange,
  reverse = false,
  label,
}: {
  axis: 'x' | 'y';
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  reverse?: boolean;
  label: string;
}) {
  const origin = useRef<{ point: number; value: number } | null>(null);
  const frame = useRef<number | null>(null);
  const pending = useRef<number | null>(null);
  const change = useRef(onChange);
  change.current = onChange;
  const flush = () => {
    if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null;
    if (pending.current !== null) {
      change.current(pending.current);
      pending.current = null;
    }
  };
  useEffect(
    () => () => {
      if (frame.current !== null) cancelAnimationFrame(frame.current);
    },
    [],
  );
  const update = (delta: number, base: number) => {
    pending.current = Math.max(min, Math.min(max, base + delta * (reverse ? -1 : 1)));
    if (frame.current === null) frame.current = requestAnimationFrame(flush);
  };
  return (
    <div
      className={`resize-handle ${axis}`}
      role="separator"
      aria-label={label}
      aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={(event) => {
        event.currentTarget.dataset.dragging = 'true';
        origin.current = { point: axis === 'x' ? event.clientX : event.clientY, value };
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (origin.current)
          update((axis === 'x' ? event.clientX : event.clientY) - origin.current.point, origin.current.value);
      }}
      onPointerUp={(event) => {
        delete event.currentTarget.dataset.dragging;
        flush();
        origin.current = null;
      }}
      onLostPointerCapture={(event) => {
        delete event.currentTarget.dataset.dragging;
        flush();
        origin.current = null;
      }}
      onKeyDown={(event) => {
        if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) {
          event.preventDefault();
          update(['ArrowRight', 'ArrowDown'].includes(event.key) ? 8 : -8, value);
          flush();
        }
      }}
    />
  );
}
