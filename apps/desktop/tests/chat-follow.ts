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
  await streamingScroll();
}

async function streamingScroll() {
  const settle = () => browser.executeAsync((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));
  const bottom = async () => {
    await browser.execute(() => {
      const view = document.querySelector('.chat-scroll')!;
      view.scrollTop = view.scrollHeight;
      view.dispatchEvent(new Event('scroll'));
      window.dispatchEvent(new Event('fixture-stream-chunk'));
    });
    await browser.waitUntil(() =>
      browser.execute(() => {
        const view = document.querySelector('.chat-scroll')!;
        return view.scrollHeight - view.clientHeight - view.scrollTop <= 2;
      }),
    );
  };
  try {
    await bottom();
    for (const delta of [-20, 10, -30, 10]) {
      const position = await browser.execute((delta) => {
        const view = document.querySelector('.chat-scroll')!;
        view.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: delta }));
        view.scrollTop += delta;
        const position = view.scrollTop;
        // Commit streamed state before the scroll handler sees the new position.
        window.dispatchEvent(new Event('fixture-stream-chunk'));
        view.dispatchEvent(new Event('scroll'));
        return position;
      }, delta);
      await settle();
      expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(position);
    }
    await bottom();
    const dragged = await browser.execute(() => {
      const view = document.querySelector('.chat-scroll')!;
      // A scrollbar/keyboard move need not produce a wheel event.
      view.scrollTop -= 15;
      const position = view.scrollTop;
      window.dispatchEvent(new Event('fixture-stream-chunk'));
      return position;
    });
    await settle();
    expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(dragged);
    await bottom();
    const position = await browser.execute(() => {
      const view = document.querySelector('.chat-scroll')!;
      window.dispatchEvent(new Event('fixture-stream-chunk'));
      // A downward gesture must also supersede an already queued follow frame.
      view.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: 10 }));
      view.scrollTop += 10;
      const position = view.scrollTop;
      view.dispatchEvent(new Event('scroll'));
      return position;
    });
    await settle();
    expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(position);
    const writes = await browser.executeAsync((done) => {
      const view = document.querySelector('.chat-scroll')!;
      const property = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop')!;
      let writes = 0;
      Object.defineProperty(view, 'scrollTop', {
        configurable: true,
        get: () => property.get!.call(view),
        set: (value) => {
          writes++;
          property.set!.call(view, value);
        },
      });
      window.dispatchEvent(new Event('fixture-stream-chunk'));
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          Reflect.deleteProperty(view, 'scrollTop');
          done(writes);
        }),
      );
    });
    expect(writes).toBe(0);
    await bottom();
  } finally {
    await browser.execute(() => window.dispatchEvent(new Event('fixture-stream-reset')));
  }
}
