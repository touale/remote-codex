import { Folder, FolderOpen } from 'lucide-react';

export function FolderIcon({ expanded }: { expanded: boolean }) {
  const Icon = expanded ? FolderOpen : Folder;
  return <Icon size={14} aria-hidden="true" />;
}
