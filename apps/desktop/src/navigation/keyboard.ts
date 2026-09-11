import type { KeyboardEvent } from 'react';
export function navigateTree(event: KeyboardEvent<HTMLElement>) {
  const label = (document.activeElement as HTMLElement)
    ?.closest<HTMLElement>('.tree-row')
    ?.querySelector<HTMLButtonElement>('.tree-label');
  if (label && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
    event.preventDefault();
    const row = label.closest('.tree-row')!;
    const toggle = row.querySelector<HTMLButtonElement>('.tree-disclosure') ?? label;
    const expanded = toggle.getAttribute('aria-expanded');
    if (expanded === (event.key === 'ArrowRight' ? 'false' : 'true')) toggle.click();
    else if (event.key === 'ArrowLeft')
      row
        .closest('.tree-node')
        ?.parentElement?.closest('.tree-node')
        ?.querySelector<HTMLButtonElement>('.tree-label')
        ?.focus();
    else row.closest('.tree-node')?.querySelector<HTMLButtonElement>('[role=group] .tree-label')?.focus();
    return;
  }
  if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
  const rows = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('.tree-label')];
  const index = label ? rows.indexOf(label) : -1;
  if (index < 0 || rows.length === 0) return;
  const target =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? rows.length - 1
        : Math.max(0, Math.min(rows.length - 1, index + (event.key === 'ArrowUp' ? -1 : 1)));
  event.preventDefault();
  rows[target]?.focus();
}
