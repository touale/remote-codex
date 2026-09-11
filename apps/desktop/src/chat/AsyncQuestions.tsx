import { Check, MessageCircleQuestion } from 'lucide-react';
import { useRef, useState } from 'react';
import { failure } from '../bridge/client';
import { ErrorText } from '../ui/controls';
import { QuestionChoice } from './QuestionChoice';
import type { Message } from './state';

export const questionReplyId = (id: string) => `question:${id}`;
export function AsyncQuestions({
  message,
  answered,
  onAnswer,
}: {
  message: Message;
  answered: boolean;
  onAnswer?: (message: Message, text: string) => Promise<void>;
}) {
  const questions = message.questions ?? [];
  const [answers, setAnswers] = useState(() => questions.map((q) => q.options[0] ?? ''));
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [error, setError] = useState('');
  return (
    <form
      className="question async-questions"
      onSubmit={(event) => {
        event.preventDefault();
        if (!onAnswer || answered || pending.current || answers.some((a) => !a.trim())) return;
        pending.current = true;
        setBusy(true);
        setError('');
        const text = questions.map((q, index) => `${q.title}\n${answers[index].trim()}`).join('\n\n');
        void onAnswer(message, text)
          .catch((e) => setError(failure(e).message))
          .finally(() => {
            pending.current = false;
            setBusy(false);
          });
      }}
    >
      <div className="question-heading">
        {answered ? <Check size={15} /> : <MessageCircleQuestion size={15} />}
        <strong>{answered ? 'Answered' : 'Your input is needed'}</strong>
      </div>
      {answered ? (
        <details>
          <summary>View questions</summary>
          {questions.map((q, i) => (
            <p key={i}>{q.title}</p>
          ))}
        </details>
      ) : (
        questions.map((q, i) => (
          <QuestionChoice
            key={i}
            label={q.title}
            options={q.options}
            value={answers[i] ?? ''}
            disabled={busy}
            onChange={(value) => setAnswers((previous) => previous.map((a, n) => (n === i ? value : a)))}
          />
        ))
      )}
      <ErrorText message={error} />
      {!answered && (
        <div className="actions">
          <button className="primary" disabled={busy || !onAnswer || answers.some((a) => !a.trim())}>
            {busy ? 'Sending…' : 'Send answers'}
          </button>
        </div>
      )}
    </form>
  );
}
