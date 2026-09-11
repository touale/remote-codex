import { ExternalLink, ShieldCheck } from 'lucide-react';
import { useRef, useState } from 'react';
import { openLink } from '../bridge/client';
import type { SessionAction } from '../bridge/session';
import { ErrorText } from '../ui/controls';
import type { Question } from './state';
import { QuestionChoice } from './QuestionChoice';

export function Questions({
  questions,
  onAction,
}: {
  questions: Question[];
  onAction: (action: SessionAction) => Promise<void>;
}) {
  return (
    <div className="questions">
      {questions.map((question) => (
        <QuestionForm key={question.id} question={question} onAction={onAction} />
      ))}
    </div>
  );
}
function QuestionForm({
  question,
  onAction,
}: {
  question: Question;
  onAction: (action: SessionAction) => Promise<void>;
}) {
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      question.interaction?.kind === 'questions'
        ? question.interaction.fields.map((f) => [f.id, f.choices[0] ?? ''])
        : [],
    ),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const interaction = question.interaction;
  const pending = useRef(false);
  const incomplete =
    interaction?.kind === 'questions' && interaction.fields.some((f) => f.required && !values[f.id]?.trim());
  const submit = async (accept: boolean) => {
    if (pending.current || (accept && incomplete)) return;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      await onAction(
        interaction
          ? { action: 'interact', request: question.id, answer: { action: accept ? 'accept' : 'cancel', values } }
          : { action: 'approve', request: question.id, decision: accept ? 'accept_once' : 'cancel' },
      );
    } catch (error) {
      setError((error as { message: string }).message);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  return (
    <form
      className="question"
      onSubmit={(event) => {
        event.preventDefault();
        void submit(true);
      }}
    >
      <div className="question-heading">
        <ShieldCheck size={16} />
        <strong>{interaction ? 'Your input is needed' : 'Approval required'}</strong>
      </div>
      <p>
        {question.description ??
          (interaction && 'message' in interaction ? interaction.message : 'Answer the questions to continue.')}
      </p>
      {interaction &&
        'fields' in interaction &&
        interaction.fields.map((field) =>
          interaction.kind === 'questions' ? (
            <QuestionChoice
              key={field.id}
              label={field.label}
              options={field.choices}
              value={values[field.id] ?? ''}
              secret={field.secret}
              disabled={busy}
              onChange={(value) => setValues((previous) => ({ ...previous, [field.id]: value }))}
            />
          ) : (
            <label className="field" key={field.id}>
              <span>
                {field.label}
                {field.required ? ' *' : ''}
              </span>
              {field.description && <small>{field.description}</small>}
              {field.kind === 'boolean' ? (
                <select
                  required={field.required}
                  value={values[field.id] ?? ''}
                  onChange={(event) => setValues({ ...values, [field.id]: event.target.value })}
                >
                  <option value="">Choose…</option>
                  <option value="true">Yes</option>
                  <option value="false">No</option>
                </select>
              ) : field.kind === 'array' ? (
                <select
                  multiple
                  value={JSON.parse(values[field.id] ?? '[]') as string[]}
                  onChange={(event) =>
                    setValues({
                      ...values,
                      [field.id]: JSON.stringify([...event.target.selectedOptions].map((o) => o.value)),
                    })
                  }
                >
                  {field.choices.map((choice) => (
                    <option key={choice}>{choice}</option>
                  ))}
                </select>
              ) : (
                <>
                  <input
                    type={field.secret ? 'password' : ['integer', 'number'].includes(field.kind) ? 'number' : 'text'}
                    step={field.kind === 'number' ? 'any' : undefined}
                    list={`choices-${question.id}-${field.id}`}
                    required={field.required}
                    value={values[field.id] ?? ''}
                    onChange={(event) => setValues({ ...values, [field.id]: event.target.value })}
                  />
                  {field.choices.length > 0 && (
                    <datalist id={`choices-${question.id}-${field.id}`}>
                      {field.choices.map((choice) => (
                        <option key={choice} value={choice} />
                      ))}
                    </datalist>
                  )}
                </>
              )}
            </label>
          ),
        )}
      {interaction?.kind === 'mcp_url' && (
        <button
          type="button"
          onClick={() => {
            void openLink(interaction.url).catch((error) => setError(error.message));
          }}
        >
          <ExternalLink size={14} /> Open authorization page
        </button>
      )}
      <ErrorText message={error} />
      <div className="actions">
        <button type="button" disabled={busy} onClick={() => void submit(false)}>
          Cancel
        </button>
        {interaction?.kind !== 'unsupported' && (
          <button className="primary" disabled={busy || incomplete}>
            {interaction ? 'Continue' : 'Allow once'}
          </button>
        )}
      </div>
    </form>
  );
}
