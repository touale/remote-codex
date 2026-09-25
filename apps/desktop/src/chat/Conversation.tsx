import { Check, Copy } from 'lucide-react';
import { memo, useEffect, useMemo, useState, type ReactNode } from 'react';
import type { TurnState } from '../bridge/session';
import type { FileChange } from '../files/useFiles';
import { IconButton } from '../ui/controls';
import { questionReplyId } from './AsyncQuestions';
import { MessageView, type PlanChoice } from './MessageView';
import type { Message } from './state';
import { clockTime, duration, fullTime } from './time';

export const Conversation = memo(function Conversation({
  messages,
  onEdit,
  onAnswer,
  editing,
  editor,
  turns,
  onDiff,
  onOpenFile,
  onPlan,
  activePlan,
  revisingPlan,
  report,
}: {
  messages: Message[];
  onEdit?: (message: Message) => void;
  onAnswer?: (message: Message, text: string) => Promise<void>;
  editing?: string;
  editor?: ReactNode;
  turns: Record<string, TurnState>;
  onDiff: (change: FileChange, newWindow?: boolean) => void;
  onOpenFile?: (href: string) => Promise<void>;
  onPlan?: (message: Message, choice: PlanChoice) => void;
  activePlan?: string;
  revisingPlan?: boolean;
  report: (error: unknown) => void;
}) {
  const groups = useMemo(() => {
    const groups = new Map<string, Message[]>();
    const ids = new Set(Object.keys(turns));
    for (const message of messages) if (message.turn) ids.add(message.turn);
    // Native turn IDs sort chronologically, as in history. Include empty turns
    // here so their stopped/failed footers cannot appear after newer progress.
    for (const id of [...ids].sort((a, b) => a.localeCompare(b))) groups.set(id, []);
    for (const message of messages) {
      const key = message.turn ?? message.id;
      const group = groups.get(key) ?? [];
      group.push(message);
      groups.set(key, group);
    }
    return groups;
  }, [messages, turns]);
  const replies = new Map(messages.filter((m) => m.role === 'user').map((m) => [m.clientId, m.text]));
  const questions = new Set(
    messages.filter((m) => m.delivery === 'async' && m.questions?.length).map((m) => questionReplyId(m.id)),
  );
  return (
    <>
      {[...groups].map(([id, messages]) => {
        const turn = turns[id];
        return (
          <section className="conversation-turn" key={id} data-turn-id={turn ? id : undefined}>
            {messages
              .filter((m) => !(m.role === 'user' && m.clientId && questions.has(m.clientId)))
              .map((message) => (
                <MessageView
                  key={message.id}
                  message={message}
                  editor={editing === message.id ? editor : undefined}
                  onEdit={
                    message.canEdit !== false && message === messages.find((m) => m.role === 'user')
                      ? onEdit
                      : undefined
                  }
                  onAnswer={onAnswer}
                  questionReply={replies.get(questionReplyId(message.id))}
                  onDiff={onDiff}
                  onOpenFile={onOpenFile}
                  report={report}
                  activePlan={message.plan && message.id === activePlan}
                  revisingPlan={revisingPlan && message.id === activePlan}
                  onPlan={onPlan}
                />
              ))}
            {turn && turn.status !== 'inProgress' && (
              <TurnFooter turn={turn} answer={answerText(messages)} report={report} />
            )}
          </section>
        );
      })}
    </>
  );
});
function answerText(messages: Message[]) {
  const texts = messages.filter((m) => m.role === 'assistant' && !m.tool && m.text);
  return [...texts].reverse().find((m) => m.phase === 'final_answer')?.text ?? texts.at(-1)?.text;
}
const TurnFooter = memo(function TurnFooter({
  turn,
  answer,
  report,
}: {
  turn: TurnState;
  answer: string | undefined;
  report: (error: unknown) => void;
}) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (copied) {
      const timer = setTimeout(() => setCopied(false), 1200);
      return () => clearTimeout(timer);
    }
  }, [copied]);
  const timing = turn.timing;
  const elapsed = timing.duration_ms == null ? null : duration(timing.duration_ms / 1000);
  return (
    <div className="turn-footer" aria-label="Turn details">
      {answer && (
        <IconButton
          label="Copy response"
          onClick={() => {
            void navigator.clipboard
              .writeText(answer)
              .then(() => setCopied(true))
              .catch(report);
          }}
        >
          {copied ? <Check size={13} /> : <Copy size={13} />}
        </IconButton>
      )}
      {timing.completed_at != null && (
        <time dateTime={new Date(timing.completed_at * 1000).toISOString()} title={fullTime(timing.completed_at)}>
          {clockTime(timing.completed_at)}
        </time>
      )}
      {elapsed && (
        <span>
          {timing.completed_at != null ? '· ' : ''}Worked for {elapsed}
        </span>
      )}
      {turn.status !== 'completed' && <span>{turn.status === 'interrupted' ? '· Stopped' : '· Failed'}</span>}
    </div>
  );
});
export function Working({ turn, waiting }: { turn: TurnState | undefined; waiting: boolean }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  const started = turn?.timing.started_at;
  return (
    <div className="working" role="status">
      <span className="pulse" />
      {waiting ? 'Waiting for your input' : 'Working'}
      {started != null && <span>· {duration(now / 1000 - started)}</span>}
    </div>
  );
}
