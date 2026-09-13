import { $, browser, expect } from '@wdio/globals';

export async function pointerAt(selector: string, axis: 'x' | 'y' | 'center' | 'outside', type = 'pointermove') {
  await browser.execute(
    (selector, axis, type) => {
      const element = document.querySelector(selector)!;
      const rect = element.getBoundingClientRect();
      (axis === 'outside' ? document.body : element).dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          pointerType: 'mouse',
          button: 0,
          pointerId: 1,
          clientX: axis === 'outside' ? 1 : axis === 'y' ? rect.right - 3 : rect.left + rect.width / 2,
          clientY: axis === 'outside' ? 1 : axis === 'x' ? rect.bottom - 3 : rect.top + rect.height / 2,
        }),
      );
    },
    selector,
    axis,
    type,
  );
}

export async function edgeScrollbar(selector: string, axis: 'x' | 'y', target = selector) {
  // Wait for async content/layout before testing a gesture on an overflowing surface.
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector, axis) => {
          const element = document.querySelector(selector)!;
          if (element.matches('.monaco-scrollable-element, .xterm-scrollable-element')) {
            const direction = axis === 'x' ? 'horizontal' : 'vertical';
            const bar = element.querySelector(`:scope > .scrollbar.${direction}`);
            const slider = bar?.querySelector('.slider');
            const dimension = axis === 'x' ? 'clientWidth' : 'clientHeight';
            return Boolean(bar && slider && slider[dimension] > 0 && slider[dimension] < bar[dimension]);
          }
          return axis === 'x'
            ? element.scrollWidth > element.clientWidth + 1
            : element.scrollHeight > element.clientHeight + 1;
        },
        selector,
        axis,
      ),
    { timeout: 15000, timeoutMsg: `${selector}: content did not overflow` },
  );
  await pointerAt(target, 'outside');
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector, axis) => !document.querySelector(selector)!.hasAttribute(`data-scroll-${axis}`),
        selector,
        axis,
      ),
    { timeout: 3000, timeoutMsg: `${selector}: scrollbar did not hide before the check` },
  );
  await browser.execute((selector) => document.querySelector(selector)!.dispatchEvent(new Event('scroll')), selector);
  await expect($(selector)).not.toHaveAttribute(`data-scroll-${axis}`);
  await pointerAt(target, axis);
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector, axis) => document.querySelector(selector)!.hasAttribute(`data-scroll-${axis}`),
        selector,
        axis,
      ),
    { timeout: 3000, timeoutMsg: `${selector}: edge did not reveal the scrollbar` },
  );
  await pointerAt(target, 'outside');
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector, axis) => !document.querySelector(selector)!.hasAttribute(`data-scroll-${axis}`),
        selector,
        axis,
      ),
    { timeout: 3000, timeoutMsg: `${selector}: scrollbar did not hide after release` },
  );
}

// Drag retention is a shared controller rule; exercise it once, not for every renderer.
export async function holdScrollbar(selector: string) {
  const held = await browser.executeAsync((selector, done) => {
    const element = document.querySelector(selector)!;
    const rect = element.getBoundingClientRect();
    const dispatch = (target: Element, type: string, x: number, y: number, buttons: number) =>
      target.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          pointerType: 'mouse',
          pointerId: 1,
          button: 0,
          buttons,
          clientX: x,
          clientY: y,
        }),
      );
    dispatch(element, 'pointerdown', rect.right - 3, rect.top + rect.height / 2, 1);
    dispatch(document.body, 'pointermove', 1, 1, 1);
    setTimeout(() => {
      const visible = element.hasAttribute('data-scroll-y');
      dispatch(document.body, 'pointerup', 1, 1, 0);
      done(visible);
    }, 900);
  }, selector);
  expect(held).toBe(true);
  await expect($(selector)).not.toHaveAttribute('data-scroll-y');
}

export async function openSelect(selector: string) {
  await $(selector).waitForDisplayed();
  await browser.execute((selector) => document.querySelector<HTMLElement>(selector)!.focus(), selector);
  await browser.keys('ArrowDown');
  await $('[role="listbox"]').waitForDisplayed();
}

export async function selectOption(selector: string, label: string) {
  await openSelect(selector);
  await browser.execute((label) => {
    const item = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
      (item) => item.textContent === label,
    );
    if (!item) throw new Error(`Missing option: ${label}`);
    item.focus();
  }, label);
  await browser.keys('Enter');
}
