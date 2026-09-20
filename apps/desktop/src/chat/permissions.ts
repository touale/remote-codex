interface PermissionState {
  full_access: boolean | null;
  approval_policy: string | null;
  reviewer: string | null;
}

export function permissionPreset(settings: PermissionState) {
  if (settings.full_access === true && settings.approval_policy === 'never') return 'full_access';
  if (settings.full_access === false && settings.approval_policy === 'on-request') return 'workspace';
  return null;
}

export function permissionLabel(settings: PermissionState) {
  if (settings.full_access === null || settings.approval_policy === null) return 'Permissions unavailable';
  const preset = permissionPreset(settings);
  if (preset === 'full_access') return 'Full Access';
  if (preset === 'workspace') {
    if (settings.reviewer === 'user') return 'Ask for approval';
    if (settings.reviewer === 'auto_review' || settings.reviewer === 'guardian_subagent') return 'Auto-review';
  }
  return 'Custom permissions';
}
