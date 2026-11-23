import { $, browser } from '@wdio/globals';

export async function enterEditorText(text: string, selector = '.monaco-editor') {
  await $(selector).waitForDisplayed();
  if (process.platform !== 'win32') {
    await $(`${selector} textarea`).addValue(text);
    return;
  }
  // Chromium uses EditContext; its hidden IME textarea does not accept editor input.
  await browser.execute(
    (selector, text) => {
      const editor = document.querySelector(selector)!;
      const input = editor.querySelector<HTMLElement>('.native-edit-context')!;
      input.focus();
      const context = Reflect.get(input, 'editContext') as EventTarget & {
        selectionStart: number;
        selectionEnd: number;
      };
      const start = context.selectionStart;
      context.dispatchEvent(
        Object.assign(new Event('textupdate'), {
          text,
          updateRangeStart: start,
          updateRangeEnd: context.selectionEnd,
          selectionStart: start + text.length,
          selectionEnd: start + text.length,
        }),
      );
    },
    selector,
    text,
  );
}
