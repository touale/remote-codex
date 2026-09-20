import { expect, it } from 'vitest';
import { permissionLabel, permissionPreset } from './permissions';

it.each([
  [true, 'never', 'user', 'Full Access', 'full_access'],
  [true, 'on-request', 'user', 'Custom permissions', null],
  [false, 'on-request', 'user', 'Ask for approval', 'workspace'],
  [false, 'on-request', 'auto_review', 'Auto-review', 'workspace'],
  [false, 'never', 'user', 'Custom permissions', null],
  [true, 'custom', 'user', 'Custom permissions', null],
  [null, null, null, 'Permissions unavailable', null],
] as const)('projects sandbox=%s policy=%s reviewer=%s', (full_access, approval_policy, reviewer, label, preset) => {
  const settings = { full_access, approval_policy, reviewer };
  expect(permissionLabel(settings)).toBe(label);
  expect(permissionPreset(settings)).toBe(preset);
});
