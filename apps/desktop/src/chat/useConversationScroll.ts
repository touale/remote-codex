import { useCallback, useLayoutEffect, useRef } from 'react';
import type { ChatState, ChatUpdate } from './state';

const INITIAL_PREFETCH_PAGES = 3;
const SCROLL_PREFETCH_PAGES = 2;
const BOTTOM_THRESHOLD = 2;

/** Owns following, reading position and bounded history prefetch for one view. */
export function useConversationScroll(
  chat: ChatState | undefined,
  update: ChatUpdate,
  load: (active?: () => boolean) => Promise<void>,
  report: (error: unknown) => void,
) {
  const scroll = useRef<HTMLDivElement>(null);
  const messages = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const previousTop = useRef(0);
  const frame = useRef<number | null>(null);
  const remaining = useRef(0);
  const pending = useRef(false);
  const lifetime = useRef({ active: false });
  const anchor = useRef<{ id: string; contentTop: number } | null>(null);
  const latest = useRef({ chat, load, report });
  latest.current = { chat, load, report };

  const remember = useCallback(() => {
    const view = scroll.current;
    anchor.current = null;
    if (!view || following.current) return;
    const top = view.getBoundingClientRect().top;
    for (const item of view.querySelectorAll<HTMLElement>('[data-message-id]')) {
      const bounds = item.getBoundingClientRect();
      if (bounds.bottom > top) {
        anchor.current = { id: item.dataset.messageId!, contentTop: bounds.top - top + view.scrollTop };
        break;
      }
    }
  }, []);
  const trackScroll = useCallback(() => {
    const view = scroll.current;
    if (!view) return false;
    const movement = view.scrollTop - previousTop.current;
    const distance = view.scrollHeight - view.clientHeight - view.scrollTop;
    if (movement < 0 && distance > BOTTOM_THRESHOLD) following.current = false;
    else if (movement > 0 && Math.abs(distance) <= BOTTOM_THRESHOLD) following.current = true;
    previousTop.current = view.scrollTop;
    return movement !== 0;
  }, []);
  const schedule = useCallback(
    function schedule() {
      if (frame.current !== null || !lifetime.current.active) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = null;
        const view = scroll.current;
        const { chat, load, report } = latest.current;
        if (!view || !view.clientHeight) return;
        // Native scrolling can precede its scroll event. Observe it before a queued follow.
        if (trackScroll()) remember();
        if (following.current && view.scrollHeight - view.clientHeight - view.scrollTop > BOTTOM_THRESHOLD) {
          view.scrollTop = view.scrollHeight - view.clientHeight;
          previousTop.current = view.scrollTop;
        }
        if (
          !chat?.historyReady ||
          !chat.nextCursor ||
          chat.historyLoading ||
          chat.historyError ||
          chat.edit ||
          chat.closed ||
          pending.current
        )
          return;
        if (!remaining.current && view.scrollTop < view.clientHeight) remaining.current = SCROLL_PREFETCH_PAGES;
        if (!remaining.current) return;
        remaining.current--;
        pending.current = true;
        const current = lifetime.current;
        void load(() => current.active)
          .catch((error) => {
            if (current.active) report(error);
          })
          .finally(() => {
            pending.current = false;
            schedule();
          });
      });
    },
    [trackScroll, remember],
  );
  useLayoutEffect(() => {
    const current = { active: true };
    lifetime.current = current;
    return () => {
      current.active = false;
    };
  }, [chat?.session.id, !!chat?.edit, chat?.closed]);
  useLayoutEffect(() => {
    const view = scroll.current;
    if (!view) return;
    const saved = latest.current.chat?.scroll;
    remaining.current = saved == null ? INITIAL_PREFETCH_PAGES : 0;
    view.scrollTop = saved ?? view.scrollHeight;
    previousTop.current = view.scrollTop;
    following.current = view.scrollHeight - view.scrollTop - view.clientHeight <= BOTTOM_THRESHOLD;
    remember();
    const observer = new ResizeObserver(schedule);
    observer.observe(view);
    if (messages.current) observer.observe(messages.current);
    schedule();
    return () => {
      observer.disconnect();
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
      update({ scroll: view.scrollTop });
    };
  }, [chat?.session.id, update, remember, schedule]);
  useLayoutEffect(() => {
    trackScroll();
    const view = scroll.current;
    const saved = anchor.current;
    let clamped = false;
    if (view && saved && !following.current && !chat?.edit) {
      const item = view.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(saved.id)}"]`);
      if (item) {
        // Content coordinates exclude the user's own movement between scroll events.
        const contentTop = item.getBoundingClientRect().top - view.getBoundingClientRect().top + view.scrollTop;
        const before = view.scrollTop;
        const top = before + contentTop - saved.contentTop;
        const bounded = Math.max(0, Math.min(top, view.scrollHeight - view.clientHeight));
        if (Math.abs(bounded - before) > 1) {
          view.scrollTop = bounded;
          previousTop.current = view.scrollTop;
        }
        // Keep the anchor if a disappearing retry row temporarily requires a
        // negative offset; the incoming page will provide room to restore it.
        clamped = Math.abs(view.scrollTop - top) > 1;
        saved.contentTop += view.scrollTop - before;
      }
    }
    if (!clamped) remember();
    schedule();
  }, [
    chat?.messages,
    chat?.turns,
    chat?.questions,
    chat?.historyReady,
    chat?.historyLoading,
    chat?.historyError,
    chat?.nextCursor,
    !!chat?.edit,
    chat?.closed,
    remember,
    schedule,
    trackScroll,
  ]);
  const onScroll = useCallback(() => {
    if (trackScroll()) remember();
    schedule();
  }, [trackScroll, remember, schedule]);
  const pauseFollow = useCallback(() => {
    following.current = false;
    remember();
  }, [remember]);
  const followLatest = useCallback(() => {
    following.current = true;
    if (scroll.current) previousTop.current = scroll.current.scrollTop;
    anchor.current = null;
    schedule();
  }, [schedule]);
  return { scroll, messages, onScroll, pauseFollow, followLatest };
}
