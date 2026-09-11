import { Activity, ArrowUp, ListTodo, Square, X } from 'lucide-react';
import type { ReactNode } from 'react';
import { useLayoutEffect, useRef, useState } from 'react';
import { failure } from '../../bridge/client';
import type { ComposerMode, SessionAction } from '../../bridge/session';
import { ErrorText, IconButton } from '../../ui/controls';
import { commandInput, suggestions, type CommandName } from '../commands';
import { changeMode } from './actions';
import { GoalProgress } from './GoalProgress';
import type { ComposerState } from './model';
import { SettingsMenu, type SettingsMenuName } from './SettingsMenu';
export function Composer({
  chat,
  setDraft,
  setComposerMode,
  preparing = false,
  ownsSubmissionDraft = false,
  focusRequest,
  onCancelPlanRevision,
  errorMessage,
  details,
  onAction,
  onStatus,
}: {
  chat: ComposerState;
  setDraft: (text: string, expected?: string) => void;
  setComposerMode: (mode: ComposerMode) => void;
  preparing?: boolean;
  ownsSubmissionDraft?: boolean;
  focusRequest?: number;
  onCancelPlanRevision?: () => void;
  errorMessage?: string | null;
  details?: ReactNode;
  onAction: (action: SessionAction) => Promise<void>;
  onStatus: () => void;
}) {
  const input = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false);
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<SettingsMenuName | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [selected, setSelected] = useState(0);
  const available = dismissed ? [] : suggestions(chat.draft);
  const ready = chat.ready;
  const settingDisabled = busy || preparing || Boolean(chat.turn) || !ready;
  const newGoal = chat.composerMode === 'goal' && (!chat.goal || chat.goal.status === 'complete');
  const revisingPlan = Boolean(onCancelPlanRevision);
  useLayoutEffect(() => {
    if (focusRequest !== undefined || revisingPlan) input.current?.focus();
  }, [focusRequest, revisingPlan]);
  useLayoutEffect(() => {
    const element = input.current;
    if (!element) return;
    element.style.height = '0px';
    element.style.height = `${Math.min(184, element.scrollHeight)}px`;
  }, [chat.draft]);
  const execute = async (operation: () => Promise<void>) => {
    if (pending.current) throw new Error('Another operation is in progress.');
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await operation();
    } catch (error) {
      if (!ownsSubmissionDraft) setError(failure(error).message);
      throw error;
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const perform = (action: SessionAction) => execute(() => onAction(action));
  const setMode = (mode: ComposerMode) =>
    execute(async () => {
      await changeMode(chat, mode, onAction);
      setComposerMode(mode);
    });
  const command = async (name: CommandName, argument = '') => {
    if (name !== 'status' && settingDisabled) {
      setError('Wait for the current operation before changing session settings.');
      return;
    }
    if (name === 'status') onStatus();
    else if (name === 'plan') await setMode(chat.composerMode === 'plan' ? 'code' : 'plan');
    else if (name === 'goal') await setMode('goal');
    else setMenu(name === 'permissions' ? 'permissions' : 'model');
    setDraft(name === 'goal' ? argument : '', chat.draft);
    setDismissed(true);
  };
  const submit = async () => {
    if (pending.current || preparing || !chat.draft.trim()) return;
    const parsed = commandInput(chat.draft);
    if (parsed) {
      await command(parsed.name, parsed.argument);
      return;
    }
    if (!ready) return;
    const text = chat.draft.trim();
    await execute(async () => {
      if (newGoal) {
        await onAction({ action: 'goal', goal: { action: 'set', objective: text, token_budget: null } });
        if (!ownsSubmissionDraft) setDraft('', chat.draft);
      } else await onAction(chat.turn ? { action: 'steer', turn: chat.turn, text } : { action: 'submit', text });
      input.current?.focus();
    });
  };
  return (
    <div className="composer-wrap">
      {chat.goal && (
        <GoalProgress
          goal={chat.goal}
          running={Boolean(chat.turn)}
          busy={busy}
          ready={ready}
          perform={perform}
          onNewGoal={async () => {
            await setMode('goal');
            input.current?.focus();
          }}
        />
      )}
      <div className="composer">
        {onCancelPlanRevision && (
          <div className="plan-revision" role="status">
            <ListTodo size={15} />
            <span>
              <strong>Refine this plan</strong>
              <small>Describe what you’d like to change.</small>
            </span>
            <IconButton label="Cancel plan revision" onClick={onCancelPlanRevision}>
              <X size={14} />
            </IconButton>
          </div>
        )}
        {available.length > 0 && (
          <div className="slash-menu" role="listbox" id="composer-commands" aria-label="Commands">
            {available.map((item, index) => (
              <button
                type="button"
                role="option"
                id={`command-${item.name}`}
                aria-selected={index === selected}
                key={item.name}
                onMouseDown={(event) => event.preventDefault()}
                onMouseEnter={() => setSelected(index)}
                onClick={() => {
                  void command(item.name).catch(() => {});
                }}
              >
                <strong>/{item.name}</strong>
                <span>{item.description}</span>
              </button>
            ))}
          </div>
        )}
        <textarea
          ref={input}
          aria-label="Message Codex"
          rows={1}
          placeholder={
            revisingPlan
              ? 'What would you like to change in the plan?'
              : newGoal
                ? 'Describe a goal…'
                : chat.composerMode === 'plan'
                  ? 'What would you like to plan?'
                  : chat.turn
                    ? 'Ask for follow-up changes…'
                    : 'Ask Codex to build, fix or explore…'
          }
          aria-controls={available.length ? 'composer-commands' : undefined}
          aria-expanded={available.length > 0}
          aria-autocomplete="list"
          aria-activedescendant={available[selected] ? `command-${available[selected].name}` : undefined}
          value={chat.draft}
          onChange={(event) => {
            setDraft(event.target.value);
            setSelected(0);
            setDismissed(false);
          }}
          onCompositionStart={() => {
            composing.current = true;
          }}
          onCompositionEnd={() => {
            composing.current = false;
          }}
          onKeyDown={(event) => {
            // WebKit can end composition before the candidate-confirming Enter.
            // That key still carries the IME marker even when isComposing is false.
            if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229 || composing.current) return;
            if (available.length && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
              event.preventDefault();
              setSelected((n) => (n + (event.key === 'ArrowDown' ? 1 : -1) + available.length) % available.length);
            } else if (event.key === 'Escape') {
              setDismissed(true);
              setError(null);
            } else if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              if (event.repeat) return;
              const operation = available[selected] ? command(available[selected].name) : submit();
              void operation.catch(() => {});
            }
          }}
        />
        <div className="composer-tools">
          <div className="composer-options">
            <IconButton label="Session status (/status)" onClick={onStatus}>
              <Activity size={16} />
            </IconButton>

            <SettingsMenu
              chat={chat}
              disabled={settingDisabled}
              menu={menu}
              setMenu={setMenu}
              setMode={setMode}
              perform={perform}
            />
          </div>
          <div className="composer-send-controls">
            {chat.turn && (
              <IconButton
                className={!chat.draft.trim() ? 'send' : ''}
                label="Stop task"
                disabled={busy}
                onClick={() => {
                  void perform({ action: 'interrupt', turn: chat.turn! }).catch(() => {});
                }}
              >
                <Square size={13} fill="currentColor" />
              </IconButton>
            )}
            {(!chat.turn || chat.draft.trim()) && (
              <IconButton
                className="send"
                label={
                  chat.turn
                    ? 'Send follow-up'
                    : revisingPlan
                      ? 'Send plan changes'
                      : newGoal
                        ? 'Start goal'
                        : 'Send message'
                }
                disabled={busy || preparing || !chat.draft.trim() || (!ready && !commandInput(chat.draft))}
                onClick={() => {
                  void submit().catch(() => {});
                }}
              >
                <ArrowUp size={17} />
              </IconButton>
            )}
          </div>
        </div>
      </div>
      {details}
      <ErrorText message={errorMessage ?? error} />
    </div>
  );
}
