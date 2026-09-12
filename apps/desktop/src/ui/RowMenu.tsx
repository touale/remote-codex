import * as Menu from '@radix-ui/react-menu';
import { useLayoutEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import type { MenuItem } from './controls';

/** The context menu and overflow button receive the same action definitions. */
export function RowMenu({
  items,
  children,
  className,
}: {
  items: MenuItem[];
  children: ReactElement;
  className?: string;
}) {
  const trigger = useRef<HTMLDivElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const interactedOutside = useRef(false);
  const [open, setOpen] = useState(false);
  const [point, setPoint] = useState({ x: 0, y: 0 });
  const anchor = useMemo(() => ({ current: { getBoundingClientRect: () => DOMRect.fromRect(point) } }), [point]);
  useLayoutEffect(() => {
    const element = trigger.current;
    if (!element) return;
    const show = (event: Event, x: number, y: number) => {
      const target = event.target instanceof Element ? event.target : null;
      // A panel owns only its background; nested rows and native text editing own theirs.
      if (
        !target ||
        target.closest('[data-context-menu]') !== element ||
        target.closest('input, textarea, [contenteditable="true"], [role="menu"]')
      )
        return;
      // Cancel the WebView menu before React's delegated listeners.
      event.preventDefault();
      event.stopPropagation();
      returnFocus.current =
        (event.target instanceof Element ? event.target.closest<HTMLElement>('button') : null) ??
        element.querySelector<HTMLElement>('[role="treeitem"]');
      interactedOutside.current = false;
      setPoint({ x, y });
      setOpen(true);
    };
    const context = (event: MouseEvent) => show(event, event.clientX, event.clientY);
    const keyboard = (event: KeyboardEvent) => {
      if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
      const bounds = element.getBoundingClientRect();
      show(event, bounds.left + 16, bounds.bottom);
    };
    element.addEventListener('contextmenu', context, { capture: true, passive: false });
    element.addEventListener('keydown', keyboard, true);
    return () => {
      element.removeEventListener('contextmenu', context, true);
      element.removeEventListener('keydown', keyboard, true);
    };
  }, []);
  return (
    <Menu.Root open={open} onOpenChange={setOpen}>
      <Menu.Anchor virtualRef={anchor} />
      <div ref={trigger} className={className} data-context-menu="" data-context-menu-open={open || undefined}>
        {children}
      </div>
      <Menu.Portal>
        <Menu.Content
          className="menu"
          side="right"
          align="start"
          sideOffset={2}
          collisionPadding={8}
          loop
          onInteractOutside={() => {
            interactedOutside.current = true;
          }}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (!interactedOutside.current && returnFocus.current?.isConnected)
              returnFocus.current.focus({ preventScroll: true });
          }}
        >
          {items.map((item) => (
            <Menu.Item
              key={item.label}
              disabled={item.disabled}
              className={`menu-item ${item.danger ? 'danger' : ''}`}
              onSelect={item.action}
            >
              {item.label}
            </Menu.Item>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}
