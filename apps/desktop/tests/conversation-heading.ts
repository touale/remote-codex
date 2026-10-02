import { $, browser, expect } from '@wdio/globals';
import { emptyStatus, emptyTiming } from '../src/bridge/session';
import type { Catalog, HistoryPage, Preferences, SessionSnapshot } from '../src/bridge/types';
import { invokeWindow } from './invoke';
import { contextAt } from './interaction-controls';

export async function conversationHeading() {
  await $('button=Settings').waitForDisplayed();
  const preferences = await invokeWindow<Preferences>('main', 'preferences', { value: null });
  const size = await browser.getWindowSize();
  const settings = {
    mode: 'agent' as const,
    model: 'fixture',
    effort: null,
    full_access: false,
    approval_policy: 'on-request',
    reviewer: 'user',
  };
  const sessions = ['first', 'second'].map((id) => ({
    id,
    title: `${id} conversation with a long title to verify compact window layout`,
    cwd: '/fixture',
    created_at: 0,
    updated_at: 0,
    archived: false,
    state: 'idle',
  }));
  const catalog: Catalog = {
    servers: [{ id: 'fixture', name: 'fixture', endpoint: { user: 'dev', host: '192.0.2.1', port: 22 } }],
    workspaces: [{ server: 'fixture', server_id: 'fixture', path: '/fixture', used_at: 0 }],
    sessions: sessions.map((session) => ({ session, server: 'fixture', server_id: 'fixture' })),
    live: [],
    open_session_ids: [],
  };
  const snapshot: SessionSnapshot = {
    goal: null,
    plan: null,
    settings,
    status: emptyStatus(),
    environment: { status: 'ready' },
    turn: null,
    current_turn: null,
    pending: [],
    closed: false,
  };
  const history: HistoryPage = {
    session: sessions[0],
    next_cursor: null,
    turns: [
      {
        id: 'turn',
        status: 'completed',
        items_before: false,
        timing: emptyTiming(),
        items: Array.from({ length: 40 }, (_, i) => ({
          id: `message-${i}`,
          kind: 'agentMessage',
          text: `Fixture paragraph ${i}.`,
          client_id: null,
          sent_at: null,
          phase: null,
        })),
      },
    ],
  };
  const install = async () => {
    await browser.execute(
      ({ catalog, snapshot, history }) => {
        const original = window.fetch;
        window.fetch = async (input, init) => {
          const url = new URL(
            typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
            location.href,
          );
          if (url.protocol !== 'ipc:') return original.call(window, input, init);
          const args = JSON.parse(init!.body as string);
          let result: unknown;
          switch (url.pathname) {
            case '/catalog':
              result = catalog;
              break;
            case '/workspace_open':
              result = { id: 'fixture', server: 'fixture', path: '/fixture' };
              break;
            case '/file_list':
              result = { entries: [], truncated: false };
              break;
            case '/session_open':
              result = {
                status: 'open',
                session: catalog.sessions.find((s) => s.session.id === args.input.resume)!.session,
                settings: snapshot.settings,
                models: [],
                snapshot,
              };
              break;
            case '/session_history':
              result = { ...history, session: catalog.sessions.find((s) => s.session.id === args.id)!.session };
              break;
            case '/session_defaults':
              result = { settings: snapshot.settings, models: [] };
              break;
            default:
              return original.call(window, input, init);
          }
          return new Response(JSON.stringify(result), {
            headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' },
          });
        };
      },
      { catalog, snapshot, history },
    );
    await $('button[aria-label="Refresh workspaces"]').click();
    await $('[aria-label="Recent sessions"] button').waitForDisplayed();
  };
  const toggle = () => $('[aria-label="Conversation heading"] button');
  const readPosition = () =>
    browser.execute(() => {
      const view = document.querySelector<HTMLElement>('.chat-scroll')!;
      return { top: view.scrollTop, height: view.clientHeight };
    });
  try {
    await browser.setWindowSize(900, 700);
    await invokeWindow('main', 'preferences', {
      value: {
        ...preferences,
        conversation_header_collapsed: false,
        sidebar_visible: true,
        workspaces_collapsed: false,
        collapsed_nodes: [],
      },
    });
    await browser.refresh();
    await $('button=Settings').waitForDisplayed();
    await expect($('[aria-label="Conversation heading"]')).not.toExist();
    await install();
    await $('[aria-label="Recent sessions"] button').click();
    await $('[data-message-id="message-0"]').waitForExist();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'true');
    const input = $('textarea[aria-label="Message Codex"]');
    await input.setValue('Keep this draft.');
    await browser.execute(() => {
      const view = document.querySelector<HTMLElement>('.chat-scroll')!;
      view.scrollTop = 200;
      view.dispatchEvent(new Event('scroll'));
    });
    const before = await readPosition();
    await toggle().click();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    await expect(toggle()).toHaveAttribute('aria-label', 'Show conversation title');
    await browser.waitUntil(async () => (await readPosition()).height > before.height);
    expect(Math.abs((await readPosition()).top - before.top)).toBeLessThanOrEqual(2);
    await expect(input).toHaveValue('Keep this draft.');
    expect(await $('[aria-label="Conversation heading"]').getSize('height')).toBe(14);
    // The embedded driver dispatches synthetic keys without native button activation.
    expect(
      await browser.execute(() => {
        const button = document.querySelector<HTMLButtonElement>('[aria-label="Conversation heading"] button')!;
        button.focus();
        return document.activeElement === button && button.tabIndex === 0;
      }),
    ).toBe(true);
    await toggle().click();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'true');
    expect(Math.abs((await readPosition()).top - before.top)).toBeLessThanOrEqual(2);
    await expect(input).toHaveValue('Keep this draft.');
    await toggle().click();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    await $('.session-row[data-session-id="second"] .tree-label').click();
    await expect(input).toHaveValue('');
    await expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    await contextAt('[data-workspace-path="/fixture"]');
    await $('[role="menuitem"]=New session').click();
    await expect($('section[aria-label="New conversation"]')).toBeDisplayed();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    await browser.waitUntil(
      async () =>
        (await invokeWindow<Preferences>('main', 'preferences', { value: null })).conversation_header_collapsed,
    );
    await browser.refresh();
    await $('button=Settings').waitForDisplayed();
    await install();
    await $('[aria-label="Recent sessions"] button').click();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    await toggle().click();
    await expect(toggle()).toHaveAttribute('aria-expanded', 'true');
  } finally {
    await invokeWindow('main', 'preferences', { value: preferences });
    await browser.refresh();
    await $('button=Settings').waitForDisplayed();
    await browser.setWindowSize(size.width, size.height);
  }
}
