import { $, browser, expect } from '@wdio/globals';

const menuItems = () =>
  browser.execute(() =>
    [...document.querySelectorAll('[role="menuitem"]')].map((node) => ({
      label: node.textContent,
      disabled: node.hasAttribute('data-disabled'),
    })),
  );
const treeSelection = () =>
  browser.execute(() =>
    [...document.querySelectorAll('.tree-label')].map((node) => ({
      title: node.getAttribute('title'),
      selected: node.getAttribute('aria-selected'),
      expanded: node.getAttribute('aria-expanded'),
    })),
  );

export async function contextAt(selector: string, bubbles = true) {
  const cancelled = await browser.execute(
    (selector, bubbles) => {
      const target = document.querySelector(selector)!;
      // Scroll and sample coordinates in one browser task: asynchronous directory
      // expansion can otherwise move the row between WebDriver calls.
      target.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      const bounds = target.getBoundingClientRect();
      const event = new MouseEvent('contextmenu', {
        bubbles,
        cancelable: true,
        button: 2,
        buttons: 2,
        clientX: bounds.left + Math.min(12, bounds.width / 2),
        clientY: bounds.top + bounds.height / 2,
      });
      target.dispatchEvent(event);
      return event.defaultPrevented;
    },
    selector,
    bubbles,
  );
  expect(cancelled).toBe(true);
  await $('[role="menu"]').waitForDisplayed();
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const bounds = document.querySelector('[role="menu"]')!.getBoundingClientRect();
        return bounds.left >= 0 && bounds.top >= 0 && bounds.right <= innerWidth && bounds.bottom <= innerHeight;
      }),
    { timeoutMsg: 'Context menu must settle inside the viewport' },
  );
}

// These are WebKit DOM-boundary checks; physical mouse/trackpad acceptance is separate.
export async function rowMenu(selector: string, expected: string, disabled?: string) {
  const before = await treeSelection();
  await browser.execute(
    (selector) => document.querySelector<HTMLElement>(`${selector} .tree-label`)!.focus(),
    selector,
  );
  // The embedded driver's click() does not emit PointerEvent, which Radix uses
  // for dropdown triggers. Exercise that event boundary explicitly.
  await browser.execute((selector) => {
    document.querySelector(`${selector} .menu-trigger`)!.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        pointerType: 'mouse',
        button: 0,
      }),
    );
  }, selector);
  await $('[role="menu"]').waitForDisplayed();
  const overflow = await menuItems();
  expect(overflow.some((item) => item.label === expected)).toBe(true);
  if (disabled) expect(overflow.find((item) => item.label === disabled)?.disabled).toBe(true);
  await browser.keys('Escape');
  await $('[role="menu"]').waitForExist({ reverse: true });

  for (const part of ['', ' .tree-label', ' .tree-label svg', ' .tree-disclosure']) {
    if (!(await $(`${selector}${part}`).isExisting())) continue;
    // Capture at the row must also handle events that never reach React's root.
    await contextAt(`${selector}${part}`, part === '');
    expect(await menuItems()).toEqual(overflow);
    expect(await treeSelection()).toEqual(before);
    await browser.keys('Escape');
    await $('[role="menu"]').waitForExist({ reverse: true });
  }
  for (const key of ['ContextMenu', 'F10']) {
    await browser.execute(
      (selector, key) => {
        const target = document.querySelector<HTMLElement>(`${selector} .tree-label`)!;
        target.focus();
        target.dispatchEvent(
          new KeyboardEvent('keydown', { key, shiftKey: key === 'F10', bubbles: true, cancelable: true }),
        );
      },
      selector,
      key,
    );
    await $('[role="menu"]').waitForDisplayed();
    expect(await menuItems()).toEqual(overflow);
    await browser.keys('Escape');
    await $('[role="menu"]').waitForExist({ reverse: true });
    expect(
      await browser.execute(
        (selector) => document.activeElement === document.querySelector(`${selector} .tree-label`),
        selector,
      ),
    ).toBe(true);
  }
}

export async function composerInput() {
  const selector = 'textarea[aria-label="Message Codex"]';
  const original = await $(selector).getValue();
  for (const text of ['中文内容还没有写完', '/status']) {
    await $(selector).setValue(text);
    const guards = await browser.execute((selector) => {
      const input = document.querySelector<HTMLTextAreaElement>(selector)!;
      const key = (values: KeyboardEventInit) => {
        const event = new KeyboardEvent('keydown', {
          key: 'Enter',
          code: 'Enter',
          keyCode: 13,
          bubbles: true,
          cancelable: true,
          ...values,
        });
        input.dispatchEvent(event);
        return event.defaultPrevented;
      };
      input.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
      const composing = key({ isComposing: true });
      input.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: input.value }));
      const committing = key({ isComposing: false, keyCode: 229 });
      input.dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter', bubbles: true }));
      const repeating = key({ repeat: true });
      const newline = key({ shiftKey: true });
      // Native input editing menus must remain available.
      const context = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
      input.dispatchEvent(context);
      return { composing, committing, repeating, newline, context: context.defaultPrevented };
    }, selector);
    expect(guards).toEqual({ composing: false, committing: false, repeating: true, newline: false, context: false });
    await expect($(selector)).toHaveValue(text);
    await expect($('[role="dialog"]')).not.toExist();
  }
  // The next intentional Enter executes /status immediately; no cooldown swallows it.
  await browser.keys('Enter');
  await $('[role="dialog"]').waitForDisplayed();
  await $('[role="dialog"] button[aria-label="Close"]').click();
  await expect($(selector)).toHaveValue('');
  await $(selector).setValue(original);
}

// The embedded driver's pointer actions synthesize click, but never dblclick.
export async function activateTree(selector: string) {
  await $(selector).waitForDisplayed();
  await browser.execute((selector) => {
    const target = document.querySelector<HTMLElement>(selector)!;
    target.focus();
    for (const detail of [1, 2])
      target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, detail }));
    target.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, button: 0, detail: 2 }));
  }, selector);
}
