import { Activity, RotateCcw } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { SessionAction } from '../bridge/session';
import type { FileChange } from '../files/useFiles';
import { Empty, ErrorText, IconButton } from '../ui/controls';
import { UsageStrip } from '../usage/UsageStrip';
import { changeMode } from './composer/actions';
import { Composer } from './composer/Composer';
import { Conversation, Working } from './Conversation';
import { Questions } from './Questions';
import { SessionStatus } from './SessionStatus';
import type { ChatState, ChatUpdate, Message } from './state';
export function Chat({
  chat,
  update,
  onAction,
  onResume,
  onHistory,
  onDiff,
  report,
}: {
  chat: ChatState | undefined;
  update: ChatUpdate;
  onAction: (action: SessionAction) => Promise<void>;
  onResume: () => void;
  onHistory: () => void;
  onDiff: (change: FileChange) => void;
  report: (error: unknown) => void;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const nearBottom = useRef(true);
  const following = useRef<number | null>(null);
  const follow = useCallback(() => {
    if (following.current !== null) return;
    following.current = requestAnimationFrame(() => {
      following.current = null;
      if (nearBottom.current && scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight;
    });
  }, []);
  const latest = useRef(chat);
  latest.current = chat;
  const [implementing, setImplementing] = useState(false);
  const [statusOpen, setStatusOpen] = useState(false);
  useLayoutEffect(() => {
    const element = scroll.current;
    const observer = new ResizeObserver(follow);
    if (element) {
      element.scrollTop = chat?.scroll ?? element.scrollHeight;
      observer.observe(element);
      nearBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 100;
    }
    return () => {
      observer.disconnect();
      if (following.current !== null) cancelAnimationFrame(following.current);
      following.current = null;
      if (element) update({ scroll: element.scrollTop });
    };
  }, [chat?.session.id, update, follow]);
  useEffect(follow, [chat?.messages, chat?.questions, chat?.turns, follow]);
  const implement = useCallback(
    (message: Message) => {
      const current = latest.current;
      if (!current || current.turn || current.closed || current.environment.status !== 'ready') return;
      setImplementing(true);
      nearBottom.current = true;
      void changeMode(current, 'code', onAction)
        .then(() => {
          update({ composerMode: 'code' });
          return onAction({ action: 'submit', text: `Implement this plan:\n\n${message.text}` });
        })
        .catch(report)
        .finally(() => setImplementing(false));
    },
    [onAction, update, report],
  );
  const execute = useCallback(
    async (action: SessionAction) => {
      if (
        action.action === 'submit' ||
        action.action === 'steer' ||
        (action.action === 'goal' && action.goal.action === 'set')
      )
        nearBottom.current = true;
      await onAction(action);
    },
    [onAction],
  );
  const showStatus = useCallback(() => setStatusOpen(true), []);
  if (!chat) return null;
  const ready = chat.environment.status === 'ready';
  return (
    <section className="chat" aria-label="Conversation">
      <div
        className="chat-scroll"
        ref={scroll}
        onScroll={(event) => {
          const element = event.currentTarget;
          nearBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 100;
        }}
      >
        <div className="messages">
          {chat.nextCursor && (
            <button className="subtle load-history" onClick={onHistory}>
              Load earlier messages
            </button>
          )}
          {chat.messages.length === 0 && !chat.turn && (
            <Empty title="What would you like to work on?" detail={`${chat.server} · ${chat.session.cwd}`} />
          )}
          <Conversation
            messages={chat.messages}
            turns={chat.turns}
            onDiff={onDiff}
            report={report}
            onImplement={chat.turn || implementing || chat.closed || !ready ? undefined : implement}
          />
          {chat.plan && (
            <details className="plan-steps" open>
              <summary>Plan progress</summary>
              {chat.plan.explanation && <p>{chat.plan.explanation}</p>}
              <ol>
                {chat.plan.steps.map((step, index) => (
                  <li key={index} data-status={step.status}>
                    {step.step}
                  </li>
                ))}
              </ol>
            </details>
          )}
          {chat.turn && <Working turn={chat.turns[chat.turn]} waiting={chat.questions.length > 0} />}
          <Questions questions={chat.questions} onAction={onAction} />
          <ErrorText message={chat.warning} />
        </div>
      </div>
      {chat.environment.status === 'action_required' && !chat.closed && (
        <div className="environment-alert" role="alert">
          <span>{chat.environment.message}</span>
          <button
            onClick={() => {
              void onAction({ action: 'retry' }).catch(report);
            }}
          >
            Retry
          </button>
        </div>
      )}
      {chat.closed ? (
        <div className="closed-session">
          <span>This session is closed.</span>
          <IconButton label="Session status (/status)" onClick={() => setStatusOpen(true)}>
            <Activity size={16} />
          </IconButton>
          <button onClick={onResume}>
            <RotateCcw size={14} />
            Resume session
          </button>
        </div>
      ) : (
        <Composer
          chat={{ ...chat, ready }}
          setDraft={(draft, expected) =>
            update((current) => ({
              ...current,
              draft: expected === undefined || current.draft === expected ? draft : current.draft,
            }))
          }
          setComposerMode={(composerMode) => update({ composerMode })}
          onAction={execute}
          onStatus={showStatus}
          details={<UsageStrip chat={chat} update={update} onDetails={showStatus} />}
        />
      )}
      {statusOpen && <SessionStatus chat={chat} update={update} report={report} onClose={() => setStatusOpen(false)} />}
    </section>
  );
}
