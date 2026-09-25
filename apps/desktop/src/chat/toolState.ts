import type { ToolItem } from '../bridge/types';

const ended = (status: string) =>
  ['completed', 'failed', 'declined', 'cancelled', 'canceled', 'interrupted'].includes(status);

// A history response may predate live output; a final aggregate is authoritative.
export function mergeTool(previous: ToolItem | undefined, incoming: ToolItem): ToolItem {
  if (!previous) return incoming;
  const current = ended(previous.status) && !ended(incoming.status) ? previous : incoming;
  const other = current === incoming ? previous : incoming;
  const output = ended(current.status)
    ? current.output || other.output
    : incoming.output.length >= previous.output.length
      ? incoming.output
      : previous.output;
  return {
    ...other,
    ...current,
    status: current.status || other.status,
    output,
    output_source: current.output_source ?? (!output ? other.output_source : undefined),
    input: current.input ?? other.input,
    links: current.links?.length ? current.links : other.links,
    changes: current.changes.length ? current.changes : other.changes,
  };
}
