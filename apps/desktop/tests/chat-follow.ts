import { browser, expect } from '@wdio/globals';

export async function chatFollow() {
  const distance = () =>
    browser.execute(() => {
      const area = document.querySelector('.chat-scroll')!;
      return area.scrollHeight - area.clientHeight - area.scrollTop;
    });
  const grow = () =>
    browser.execute(() => {
      const content = document.querySelector<HTMLElement>('[data-follow-probe]')!;
      content.style.height = `${content.offsetHeight + 350}px`;
      // WebKit may deliver an earlier scroll event after content has already grown.
      document.querySelector('.chat-scroll')!.dispatchEvent(new Event('scroll'));
    });
  await browser.execute(() => {
    const content = document.createElement('div');
    content.dataset.followProbe = '';
    content.style.cssText = 'height:1200px;flex:none';
    content.textContent = 'Delayed message layout';
    document.querySelector('.messages')!.append(content);
    const area = document.querySelector('.chat-scroll')!;
    area.scrollTop = area.scrollHeight;
    area.dispatchEvent(new Event('scroll'));
  });
  try {
    await browser.waitUntil(async () => (await distance()) < 2);
    await grow();
    await browser.waitUntil(async () => (await distance()) < 2);
    const reading = await browser.execute(() => {
      const area = document.querySelector('.chat-scroll')!;
      area.scrollTop -= 400;
      area.dispatchEvent(new Event('scroll'));
      return area.scrollTop;
    });
    await grow();
    await browser.executeAsync((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));
    expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(reading);
    await browser.execute(() => {
      const area = document.querySelector('.chat-scroll')!;
      area.scrollTop = area.scrollHeight;
      area.dispatchEvent(new Event('scroll'));
    });
    await grow();
    await browser.waitUntil(async () => (await distance()) < 2);
  } finally {
    await browser.execute(() => document.querySelector('[data-follow-probe]')?.remove());
  }
}
