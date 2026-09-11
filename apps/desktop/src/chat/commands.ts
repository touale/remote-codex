const commands = [
  { name: 'status', description: 'Session, context and usage limits' },
  { name: 'plan', description: 'Switch between planning and coding' },
  { name: 'goal', description: 'Work toward a longer-running goal' },
  { name: 'model', description: 'Choose a model and reasoning effort' },
  { name: 'reasoning', description: 'Adjust how deeply Codex reasons' },
  { name: 'permissions', description: 'Choose when Codex asks for approval' },
] as const;
export type CommandName = (typeof commands)[number]['name'];
export function commandInput(draft: string): { name: CommandName; argument: string } | null {
  const match = /^\/(\w+)(?:\s+([\s\S]*))?$/.exec(draft.trim());
  if (!match || !commands.some((c) => c.name === match[1])) return null;
  const argument = match[2]?.trim() ?? '';
  if (argument && match[1] !== 'goal') return null;
  return { name: match[1] as CommandName, argument };
}
export function suggestions(draft: string) {
  const match = /^\/(\w*)$/.exec(draft);
  return match ? commands.filter((c) => c.name.startsWith(match[1])) : [];
}
