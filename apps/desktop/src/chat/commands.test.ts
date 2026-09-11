import { expect, it } from 'vitest';
import { commandInput, suggestions } from './commands';
it('recognizes only implemented commands and preserves remote paths as text', () => {
  expect(commandInput('/status')).toEqual({ name: 'status', argument: '' });
  expect(commandInput('/goal Review the project')).toEqual({ name: 'goal', argument: 'Review the project' });
  for (const input of ['/workspace/test', '/status.json', '/unknown', '/model is a path', 'Explain /status'])
    expect(commandInput(input)).toBeNull();
  expect(suggestions('/sta').map((c) => c.name)).toEqual(['status']);
  expect(suggestions('/workspace/test')).toEqual([]);
});
