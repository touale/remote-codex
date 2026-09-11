import { useId } from 'react';

/** Choice labels stay visible; free text is available without a synthetic option. */
export function QuestionChoice({
  label,
  options,
  value,
  onChange,
  disabled,
  secret = false,
}: {
  label: string;
  options: string[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  secret?: boolean;
}) {
  const name = useId();
  return (
    <fieldset className="question-choice" disabled={disabled}>
      <legend>{label}</legend>
      {options.map((option) => (
        <label className="question-option" key={option}>
          <input type="radio" name={name} checked={value === option} onChange={() => onChange(option)} />
          <span>{option}</span>
        </label>
      ))}
      <input
        aria-label={`${label} — your answer`}
        type={secret ? 'password' : 'text'}
        placeholder={options.length ? 'Or write your own answer…' : 'Your answer…'}
        value={options.includes(value) ? '' : value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229))
            event.preventDefault();
        }}
      />
    </fieldset>
  );
}
