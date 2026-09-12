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
import { MessageEditor, startEdit } from './MessageEditor';
import { questionReplyId } from './AsyncQuestions';
import { SessionStatus } from './SessionStatus';
import type { PlanChoice } from './MessageView';
import type { ChatState, ChatUpdate, Message } from './state';
export function Chat({
  chat,
  update,
  onAction,
  onResume,
  onFreshPlan,
  onHistory,
  onRevert,
  onReloadEdit,
  onDiff,
  report,
}: {
  chat: ChatState | undefined;
  update: ChatUpdate;
  onAction: (action: SessionAction) => Promise<void>;
  onResume: () => void;
  onFreshPlan: (chat: ChatState, plan: Message) => Promise<void>;
  onHistory: () => void;
  onRevert: (turn: string) => Promise<void>;
  onReloadEdit: () => Promise<void>;
  onDiff: (change: FileChange, newWindow?: boolean) => void;
  report: (error: unknown) => void;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const messages = useRef<HTMLDivElement>(null);
  const nearBottom = useRef(true);
  const previousTop = useRef(0);
  const following = useRef<number | null>(null);
  const follow = useCallback(() => {
    if (following.current !== null) return;
    following.current = requestAnimationFrame(() => {
      following.current = null;
      if (nearBottom.current && scroll.current) {
        scroll.current.scrollTop = scroll.current.scrollHeight;
        previousTop.current = scroll.current.scrollTop;
      }
    });
  }, []);
  const latest = useRef(chat);
  latest.current = chat;
  // Reject a second click before React renders the disabled controls.
  const pendingPlan = useRef(false);
  const [choosingPlan, setChoosingPlan] = useState(false);
  const [revisingPlan, setRevisingPlan] = useState<string | null>(null);
  const [statusOpen, setStatusOpen] = useState(false);
  useLayoutEffect(() => {
    const element = scroll.current;
    const observer = new ResizeObserver(follow);
    if (element) {
      element.scrollTop = chat?.scroll ?? element.scrollHeight;
      previousTop.current = element.scrollTop;
      observer.observe(element);
      if (messages.current) observer.observe(messages.current);
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
  const choosePlan = useCallback(
    async (message: Message, choice: PlanChoice) => {
      const current = latest.current;
      if (!current || current.turn || current.closed || current.environment.status !== 'ready' || pendingPlan.current)
        return;
      pendingPlan.current = true;
      setChoosingPlan(true);
      try {
        if (choice === 'fresh') {
          await onFreshPlan(current, message);
          return;
        }
        const mode = choice === 'revise' ? 'plan' : 'code';
        await changeMode(current, mode, onAction);
        update({ composerMode: mode });
        if (choice === 'revise') {
          setRevisingPlan(message.id);
        } else {
          nearBottom.current = true;
          await onAction({ action: 'submit', text: 'Implement the plan.' });
        }
      } catch (error) {
        report(error);
      } finally {
        pendingPlan.current = false;
        setChoosingPlan(false);
      }
    },
    [onAction, onFreshPlan, update, report],
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
  const editReady = chat.environment.status === 'ready' && !chat.closed && !chat.turn && chat.goal?.status !== 'active';
  const editor = chat.edit ? (
    <MessageEditor
      edit={chat.edit}
      update={update}
      enabled={editReady}
      onRevert={onRevert}
      onReload={onReloadEdit}
      onSend={(text, clientId) => execute({ action: 'submit', text, client_id: clientId })}
    />
  ) : null;
  const ready = chat.environment.status === 'ready';
  const latestPlan = chat.messages.filter((message) => message.plan || message.role === 'user').at(-1);
  const revision = Boolean(
    latestPlan?.plan && latestPlan.id === revisingPlan && chat.composerMode === 'plan' && !chat.turn,
  );
  return (
    <section className="chat" aria-label="Conversation">
      <div
        className="chat-scroll"
        ref={scroll}
        onScroll={(event) => {
          const element = event.currentTarget;
          const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 100;
          // Content growth can emit a scroll event before the next follow frame.
          // Only an upward move leaves follow mode; returning to the bottom restores it.
          if (atBottom || element.scrollTop < previousTop.current) nearBottom.current = atBottom;
          previousTop.current = element.scrollTop;
        }}
      >
        <div className="messages" ref={messages}>
          {chat.nextCursor && (
            <button className="subtle load-history" disabled={!!chat.edit} onClick={onHistory}>
              Load earlier messages
            </button>
          )}
          {chat.messages.length === 0 && !chat.turn && (
            <Empty title="What would you like to work on?" detail={`${chat.server} · ${chat.session.cwd}`} />
          )}
          <Conversation
            messages={chat.messages}
            editing={chat.edit?.id}
            editor={editor}
            onEdit={
              editReady && !chat.edit ? (message) => update((current) => startEdit(current, message.id)) : undefined
            }
            onAnswer={
              ready && !chat.closed && !chat.edit
                ? async (message, text) => {
                    const current = latest.current;
                    if (!current) return;
                    await execute(
                      current.turn
                        ? { action: 'steer', turn: current.turn, text, client_id: questionReplyId(message.id) }
                        : { action: 'submit', text, client_id: questionReplyId(message.id) },
                    );
                  }
                : undefined
            }
            turns={chat.turns}
            activePlan={latestPlan?.plan ? latestPlan.id : undefined}
            revisingPlan={revision}
            onDiff={onDiff}
            report={report}
            onPlan={chat.turn || choosingPlan || chat.closed || chat.edit || !ready ? undefined : choosePlan}
          />
          {chat.edit && !chat.messages.some((m) => m.id === chat.edit?.id) && editor}
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
          preparing={choosingPlan || Boolean(chat.edit)}
          onCancelPlanRevision={revision ? () => setRevisingPlan(null) : undefined}
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
