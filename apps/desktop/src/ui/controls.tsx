import { MoreHorizontal, X } from 'lucide-react';
import { Dialog, DropdownMenu, Tooltip } from 'radix-ui';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

export function IconButton({ label, children, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button {...props} className={`icon-button ${props.className ?? ''}`} aria-label={label}>
          {children}
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content className="tooltip" sideOffset={5}>
          {label}
          <Tooltip.Arrow />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
export function Modal({
  title,
  description,
  children,
  onClose,
  closeDisabled = false,
  wide = false,
  className = '',
}: {
  title: string;
  description?: string;
  children: ReactNode;
  onClose: () => void;
  closeDisabled?: boolean;
  wide?: boolean;
  className?: string;
}) {
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !closeDisabled) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className={`modal ${wide ? 'wide' : ''} ${className}`}
          onInteractOutside={(event) => event.preventDefault()}
        >
          <div className="modal-header">
            <Dialog.Title>{title}</Dialog.Title>
            <Dialog.Close asChild>
              <button className="icon-button" aria-label="Close" disabled={closeDisabled}>
                <X size={16} />
              </button>
            </Dialog.Close>
          </div>
          <Dialog.Description className={description ? 'muted' : 'sr-only'}>{description ?? title}</Dialog.Description>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export interface MenuItem {
  label: string;
  action: () => void;
  danger?: boolean;
  disabled?: boolean;
}
export function Menu({ items, label = 'More actions' }: { label?: string; items: MenuItem[] }) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button className="icon-button menu-trigger" aria-label={label}>
          <MoreHorizontal size={16} />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu" sideOffset={4} align="end">
          {items.map((item, index) => (
            <DropdownMenu.Item
              key={index}
              disabled={item.disabled}
              className={`menu-item ${item.danger ? 'danger' : ''}`}
              onSelect={item.action}
            >
              {item.label}
            </DropdownMenu.Item>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
export function ErrorText({ message }: { message: string | null | undefined }) {
  return message ? (
    <div className="inline-error" role="alert">
      {message}
    </div>
  ) : null;
}
export function Empty({ title, detail, children }: { title: string; detail?: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {detail && <p>{detail}</p>}
      {children}
    </div>
  );
}
