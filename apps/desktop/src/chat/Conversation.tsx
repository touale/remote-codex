import { Check, Copy } from 'lucide-react';
import { memo, useEffect, useMemo, useState } from 'react';
import type { TurnState } from '../bridge/session';
import type { FileChange } from '../files/useFiles';
import { IconButton } from '../ui/controls';
import { MessageView, type PlanChoice } from './MessageView';
import type { Message } from './state';
import { clockTime, duration, fullTime } from './time';

export const Conversation = memo(function Conversation({
  messages,
  turns,
  onDiff,
  onPlan,
  activePlan,
  revisingPlan,
  report,
}: {
  messages: Message[];
  turns: Record<string, TurnState>;
  onDiff: (change: FileChange) => void;
  onPlan?: (message: Message, choice: PlanChoice) => void;
  activePlan?: string;
  revisingPlan?: boolean;
  report: (error: unknown) => void;
}) {
  const groups = useMemo(() => {
    const groups = new Map<string, Message[]>();
    for (const message of messages) {
      const key = message.turn ?? message.id;
      const group = groups.get(key) ?? [];
      group.push(message);
      groups.set(key, group);
    }
    // Native turns without visible output still need an interrupted/failed footer.
    for (const turn of Object.values(turns)) if (!groups.has(turn.id)) groups.set(turn.id, []);
    return groups;
  }, [messages, turns]);
  return (
    <>
      {[...groups].map(([id, messages]) => {
        const turn = turns[id];
        return (
          <section className="conversation-turn" key={id} data-turn-id={turn ? id : undefined}>
            {messages.map((message) => (
              <MessageView
                key={message.id}
                message={message}
                onDiff={onDiff}
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
