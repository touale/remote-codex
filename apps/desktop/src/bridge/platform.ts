export const isMac = /Mac/i.test(navigator.platform);
export const modifierKey = isMac ? '⌘' : 'Ctrl+';
document.documentElement.dataset.platform = isMac ? 'macos' : 'other';
