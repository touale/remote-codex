import { ChevronRight } from 'lucide-react';
import type { ReactNode } from 'react';
import './disclosure.css';

export function Disclosure({ title, children }: { title: string; children: ReactNode }) {
  return (
    <details className="disclosure">
      <summary>
        <span>{title}</span>
        <ChevronRight size={14} aria-hidden="true" />
      </summary>
      <div className="disclosure-content">{children}</div>
    </details>
  );
}
