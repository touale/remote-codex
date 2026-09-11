import { Check, ChevronDown } from 'lucide-react';
import { Select } from 'radix-ui';
import './select.css';

interface SelectOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}
export function SelectField({
  id,
  label,
  value,
  options,
  placeholder = 'Choose…',
  disabled,
  required,
  onValueChange,
}: {
  id?: string;
  label: string;
  value: string;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  required?: boolean;
  onValueChange: (value: string) => void;
}) {
  const selected = options.find((option) => option.value === value);
  return (
    <Select.Root value={value} onValueChange={onValueChange} disabled={disabled} required={required}>
      <Select.Trigger id={id} className="select-field" aria-label={label} title={selected?.label}>
        <Select.Value placeholder={placeholder} />
        <Select.Icon asChild>
          <ChevronDown size={14} />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content className="select-menu" position="popper" sideOffset={5} collisionPadding={12} align="start">
          <Select.Viewport className="select-viewport">
            {options.map((option) => (
              <Select.Item
                key={option.value}
                value={option.value}
                textValue={option.label}
                disabled={option.disabled}
                className="select-option"
                title={option.label}
              >
                <span className="select-option-label">
                  <Select.ItemText>{option.label}</Select.ItemText>
                  {option.description && <small>{option.description}</small>}
                </span>
                <Select.ItemIndicator>
                  <Check size={14} />
                </Select.ItemIndicator>
              </Select.Item>
            ))}
            {!options.length && <div className="select-empty">No options available</div>}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}
