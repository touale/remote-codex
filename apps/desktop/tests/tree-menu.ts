import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { contextAt } from './interaction-controls';
import { pointerAt } from './scroll-controls';

async function background(selector: string, token?: '--hover' | '--selected') {
  return browser.execute(
    (selector, token) => {
      const row = document.querySelector<HTMLElement>(selector)!;
      if (!token) return getComputedStyle(row).backgroundColor;
      // Resolve theme tokens through the browser, including color serialization.
      const probe = document.createElement('span');
      probe.style.backgroundColor = `var(${token})`;
      row.append(probe);
      const color = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return color;
    },
    selector,
    token,
  );
}

export async function treeMenuHighlight(setTheme: (theme: 'light' | 'dark') => Promise<unknown>) {
  await browser.setWindowSize(1100, 800);
  await $('button=Fixture activity').click();
  const selected = '#activity-tree [data-session-id="recent"]';
  const server = '#activity-tree .server-row';
  const session = '#activity-tree [data-session-id="old"]';
  await $(`${selected} .tree-label`).click();
  await expect($(selected)).toHaveElementClass('selected');
  for (const theme of ['light', 'dark'] as const) {
    await setTheme(theme);
    await expect($('html')).toHaveAttribute('data-theme', theme);
    for (const row of [server, session, selected]) {
      const expected = await background(row, row === selected ? '--selected' : '--hover');
      await contextAt(row);
      await $('[role="menuitem"]').moveTo();
      expect(await background(row)).toBe(expected);
      await expect($(`${selected} .tree-label`)).toHaveAttribute('aria-selected', 'true');
      await expect($(`${server} .tree-label`)).toHaveAttribute('aria-selected', 'false');
      await expect($(`${session} .tree-label`)).toHaveAttribute('aria-selected', 'false');
      if (row === session)
        await browser.saveScreenshot(path.resolve(`../../.artifacts/desktop-e2e/tree-menu-${theme}.png`));
      await browser.keys('Escape');
      await $('[role="menu"]').waitForExist({ reverse: true });
      await $('button=Fixture settings').moveTo();
      expect(await background(row)).toBe(row === selected ? expected : 'rgba(0, 0, 0, 0)');

      await browser.execute((row) => {
        document
          .querySelector(`${row} .menu-trigger`)!
          .dispatchEvent(
            new PointerEvent('pointerdown', { bubbles: true, cancelable: true, pointerType: 'mouse', button: 0 }),
          );
      }, row);
      await $('[role="menu"]').waitForDisplayed();
      await $('[role="menuitem"]').moveTo();
      expect(await background(row)).toBe(expected);
      await pointerAt('#activity-tree', 'outside', 'pointerdown');
      await $('[role="menu"]').waitForExist({ reverse: true });
      await $('button=Fixture settings').moveTo();
      expect(await background(row)).toBe(row === selected ? expected : 'rgba(0, 0, 0, 0)');
    }
    // An action dismisses its highlight, and a background menu never highlights descendant rows.
    await contextAt(server);
    await $('[role="menuitem"]=Refresh').click();
    await $('[role="menu"]').waitForExist({ reverse: true });
    await $('button=Fixture settings').moveTo();
    await contextAt('#activity-tree .tree-scroll');
    expect(await background(server)).toBe('rgba(0, 0, 0, 0)');
    expect(await background(session)).toBe('rgba(0, 0, 0, 0)');
    expect(await background(selected)).toBe(await background(selected, '--selected'));
    await browser.keys('Escape');
  }
  await $('button=Fixture activity').click();
}
