import { Pencil } from 'lucide-react';
import { memo, type ReactNode } from 'react';
import { MarkdownBody } from './MarkdownBody';
import { ToolMessage } from './ToolMessage';
import type { FileChange } from '../files/useFiles';
import type { Message } from './state';
import { QuestionResult } from './QuestionResult';
import { AsyncQuestions } from './AsyncQuestions';
import { clockTime, fullTime } from './time';
export type PlanChoice = 'implement' | 'fresh' | 'revise';
export const MessageView = memo(function MessageView({
  message,
  onEdit,
  editor,
  onAnswer,
  questionReply,
  onDiff,
  report,
  onPlan,
  activePlan,
  revisingPlan,
}: {
  message: Message;
  onEdit?: (message: Message) => void;
  editor?: ReactNode;
  onAnswer?: (message: Message, text: string) => Promise<void>;
  questionReply?: string;
  onPlan?: (message: Message, choice: PlanChoice) => void;
  activePlan?: boolean;
  revisingPlan?: boolean;
  onDiff: (change: FileChange) => void;
  report: (error: unknown) => void;
}) {
  if (message.role === 'user' && message.clientId?.startsWith('question:'))
    return <QuestionResult text={message.text} />;
  if (message.tool) return <ToolMessage tool={message.tool} onDiff={onDiff} report={report} />;
  if (editor) return <article className="message user editing-message">{editor}</article>;
  if (message.delivery === 'async' && message.questions?.length)
    return <AsyncQuestions message={message} reply={questionReply} onAnswer={onAnswer} />;
  return (
    <article className={`message ${message.role} ${message.plan ? 'plan-card' : ''}`}>
      <div className="message-body">
        <MarkdownBody text={message.text} report={report} />
      </div>
      {message.plan && message.complete && (
        <div className="plan-actions">
          <small>{revisingPlan ? 'Continuing in Plan mode' : 'Proposed plan'}</small>
          {activePlan && !revisingPlan && (
            <div className="plan-choices" role="group" aria-label="Plan next steps">
              <button className="primary" disabled={!onPlan} onClick={() => onPlan?.(message, 'implement')}>
                Implement plan
              </button>
              <button
                disabled={!onPlan}
                title="Start a new session with this plan. This conversation is kept."
                onClick={() => onPlan?.(message, 'fresh')}
              >
                Clear context and implement
              </button>
              <button disabled={!onPlan} onClick={() => onPlan?.(message, 'revise')}>
                Continue planning
              </button>
            </div>
          )}
        </div>
      )}
      {onEdit && message.role === 'user' && message.turn && (
        <button className="edit-message" aria-label="Edit message" title="Edit message" onClick={() => onEdit(message)}>
          <Pencil size={13} />
        </button>
      )}
      {message.role === 'user' && message.sentAt != null && (
        <time
          className="message-time"
          dateTime={new Date(message.sentAt * 1000).toISOString()}
          title={fullTime(message.sentAt)}
        >
          {clockTime(message.sentAt)}
        </time>
      )}
    </article>
  );
});
