import { useCallback, useLayoutEffect, useRef } from 'react';
import type { ChatState, ChatUpdate } from './state';

const INITIAL_PREFETCH_PAGES = 3;
const SCROLL_PREFETCH_PAGES = 2;
const BOTTOM_THRESHOLD = 100;

/** Owns following, reading position and bounded history prefetch for one view. */
export function useConversationScroll(
  chat: ChatState | undefined,
  update: ChatUpdate,
  load: (active?: () => boolean) => Promise<void>,
  report: (error: unknown) => void,
) {
  const scroll = useRef<HTMLDivElement>(null);
  const messages = useRef<HTMLDivElement>(null);
  const nearBottom = useRef(true);
  const previousTop = useRef(0);
  const frame = useRef<number | null>(null);
  const remaining = useRef(0);
  const pending = useRef(false);
  const lifetime = useRef({ active: false });
  const anchor = useRef<{ id: string; top: number } | null>(null);
  const latest = useRef({ chat, load, report });
  latest.current = { chat, load, report };

  const remember = useCallback(() => {
    const view = scroll.current;
    anchor.current = null;
    if (!view || nearBottom.current) return;
    const top = view.getBoundingClientRect().top;
    for (const item of view.querySelectorAll<HTMLElement>('[data-message-id]')) {
      const bounds = item.getBoundingClientRect();
      if (bounds.bottom > top) {
        anchor.current = { id: item.dataset.messageId!, top: bounds.top - top };
        break;
      }
    }
  }, []);
  const schedule = useCallback(function schedule() {
    if (frame.current !== null || !lifetime.current.active) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = null;
      const view = scroll.current;
      const { chat, load, report } = latest.current;
      if (!view || !view.clientHeight) return;
      if (nearBottom.current) {
        view.scrollTop = view.scrollHeight;
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
  }, []);
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
    nearBottom.current = view.scrollHeight - view.scrollTop - view.clientHeight < BOTTOM_THRESHOLD;
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
    const view = scroll.current;
    const saved = anchor.current;
    let clamped = false;
    if (view && saved && !nearBottom.current && !chat?.edit) {
      const item = view.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(saved.id)}"]`);
      if (item) {
        const top = view.scrollTop + item.getBoundingClientRect().top - view.getBoundingClientRect().top - saved.top;
        view.scrollTop = top;
        previousTop.current = view.scrollTop;
        // Keep the anchor if a disappearing retry row temporarily requires a
        // negative offset; the incoming page will provide room to restore it.
        clamped = Math.abs(view.scrollTop - top) > 1;
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
  ]);
  const onScroll = useCallback(() => {
    const view = scroll.current;
    if (!view) return;
    const moved = view.scrollTop !== previousTop.current;
    const atBottom = view.scrollHeight - view.scrollTop - view.clientHeight < BOTTOM_THRESHOLD;
    // Content growth may emit a scroll event before the next follow frame.
    if (atBottom || view.scrollTop < previousTop.current) nearBottom.current = atBottom;
    previousTop.current = view.scrollTop;
    if (moved) remember();
    schedule();
  }, [remember, schedule]);
  const followLatest = useCallback(() => {
    nearBottom.current = true;
    anchor.current = null;
    schedule();
  }, [schedule]);
  return { scroll, messages, onScroll, followLatest };
}
