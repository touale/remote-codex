import { $, browser } from '@wdio/globals';

export async function pasteInEditor(text: string, selector = '.monaco-editor') {
  await $(selector).waitForDisplayed();
  // Chromium uses EditContext; its hidden IME textarea does not accept editor input.
  await browser.execute(
    (selector, text) => {
      const editor = document.querySelector(selector)!;
      const input = editor.querySelector<HTMLElement>('.native-edit-context, textarea')!;
      input.focus();
      const clipboardData = new DataTransfer();
      clipboardData.setData('text/plain', text);
      input.dispatchEvent(new ClipboardEvent('paste', { clipboardData, bubbles: true, cancelable: true }));
    },
    selector,
    text,
  );
}
