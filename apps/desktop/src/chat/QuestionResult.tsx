import { Check } from 'lucide-react';
import type { AsyncQuestion } from '../bridge/types';

// Async replies are native user input. Recover their fields for the question
// result view without changing stored messages or the model's conversation.
function questionAnswers(questions: AsyncQuestion[], text: string): string[] | null {
  let remaining = text;
  const answers: string[] = [];
  for (const [i, question] of questions.entries()) {
    const prefix = `${question.title}\n`;
    if (!remaining.startsWith(prefix)) return null;
    remaining = remaining.slice(prefix.length);
    const next = questions[i + 1];
    const end = next ? remaining.indexOf(`\n\n${next.title}\n`) : remaining.length;
    if (end < 0) return null;
    answers.push(remaining.slice(0, end).trim());
    remaining = next ? remaining.slice(end + 2) : '';
  }
  return answers;
}
export function QuestionResult({ questions = [], text }: { questions?: AsyncQuestion[]; text: string }) {
  const answers = questions.length ? questionAnswers(questions, text) : null;
  const count = answers?.filter(Boolean).length;
  return (
    <section className="question question-result" aria-label="Question results">
      <div className="question-heading">
        <Check size={15} />
        <strong>Questions</strong>
        <small>{count === undefined ? 'Answered' : `${count}/${questions.length} answered`}</small>
      </div>
      {answers ? (
        <dl>
          {questions.map((q, i) => (
            <div key={i}>
              <dt>{q.title}</dt>
              <dd>{answers[i] || 'Unanswered'}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="question-response-text">{text}</p>
      )}
    </section>
  );
}
