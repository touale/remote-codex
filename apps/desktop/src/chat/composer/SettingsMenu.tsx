import { Check, ChevronDown, Code2, ListTodo, ShieldCheck, Target } from 'lucide-react';
import { Popover } from 'radix-ui';
import { useState } from 'react';
import type { ComposerMode, SessionAction } from '../../bridge/session';
import { Modal } from '../../ui/controls';
import { permissionLabel } from '../permissions';
import type { ComposerState } from './model';
export type SettingsMenuName = 'mode' | 'model' | 'permissions';
const modeLabels = { code: 'Code', plan: 'Plan', goal: 'Goal' };
export function SettingsMenu({
  chat,
  disabled,
  menu,
  setMenu,
  setMode,
  perform,
}: {
  chat: ComposerState;
  disabled: boolean;
  menu: SettingsMenuName | null;
  setMenu: (menu: SettingsMenuName | null) => void;
  setMode: (mode: ComposerMode) => Promise<void>;
  perform: (action: SessionAction) => Promise<void>;
}) {
  const [confirmFull, setConfirmFull] = useState(false);
  const [custom, setCustom] = useState('');
  const [customVisible, setCustomVisible] = useState(false);
  const permission = permissionLabel(chat.settings);
  const model = chat.models.find((m) => m.id === chat.settings.model);
  const ModeIcon = { code: Code2, plan: ListTodo, goal: Target }[chat.composerMode];
  const settings = (value: Extract<SessionAction, { action: 'settings' }>['settings']) =>
    perform({ action: 'settings', settings: value });
  const closeAfter = (action: Promise<void>) => {
    void action.then(() => setMenu(null)).catch(() => {});
  };
  const options = (name: SettingsMenuName, label: string, content: React.ReactNode, icon?: React.ReactNode) => (
    <Popover.Root
      open={menu === name}
      onOpenChange={(open) => {
        setMenu(open ? name : null);
      }}
    >
      <Popover.Trigger asChild>
        <button
          className="composer-chip"
          aria-label={name === 'mode' ? 'Session mode' : name === 'model' ? 'Model and reasoning' : 'Permissions'}
          disabled={disabled || (name !== 'mode' && chat.settingsState !== undefined && chat.settingsState !== 'ready')}
          title={label}
        >
          {icon}
          {name !== 'mode' && chat.settingsState === 'loading' ? (
            <span className="settings-skeleton" aria-label="Loading settings" />
          ) : (
            <span>{label}</span>
          )}
          <ChevronDown size={11} />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          className={`composer-menu ${name}-menu`}
          side="top"
          align="start"
          sideOffset={10}
          aria-label={
            name === 'mode' ? 'Choose mode' : name === 'model' ? 'Choose model and reasoning' : 'Choose permissions'
          }
        >
          {content}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
  return (
    <>
      {options(
        'mode',
        modeLabels[chat.composerMode],
        <div role="group" aria-label="Mode">
          {(['code', 'plan', 'goal'] as const).map((mode) => {
            const Icon = { code: Code2, plan: ListTodo, goal: Target }[mode];
            return (
              <button
                key={mode}
                className="composer-choice"
                aria-pressed={chat.composerMode === mode}
                disabled={disabled}
                onClick={() => closeAfter(setMode(mode))}
              >
                <Icon size={16} />
                <span>
                  <strong>{modeLabels[mode]}</strong>
                  <small>
                    {
                      {
                        code: 'Make changes and get things done',
                        plan: 'Think through an approach first',
                        goal: 'Keep working toward an objective',
                      }[mode]
                    }
                  </small>
                </span>
                {chat.composerMode === mode && <Check size={14} />}
              </button>
            );
          })}
        </div>,
        <ModeIcon size={14} />,
      )}
      {options(
        'model',
        `${model?.name ?? chat.settings.model ?? 'Model unavailable'}${chat.settings.effort ? ` · ${chat.settings.effort}` : ''}`,
        <>
          <div className="menu-heading">Model</div>
          <div className="model-choices" role="group" aria-label="Model">
            {chat.models.map((option) => (
              <button
                key={option.id}
                className="composer-choice"
                disabled={disabled}
                aria-pressed={chat.settings.model === option.id}
                onClick={() => {
                  const effort =
                    chat.settings.effort && option.efforts.includes(chat.settings.effort)
                      ? chat.settings.effort
                      : option.default_effort;
                  void settings({ model: option.id, effort }).catch(() => {});
                }}
              >
                <span>
                  <strong>{option.name}</strong>
                  <small>{option.description}</small>
                </span>
                {chat.settings.model === option.id && <Check size={14} />}
              </button>
            ))}
          </div>
          <button
            className="composer-choice"
            onClick={() => {
              setCustomVisible(!customVisible);
              setCustom(chat.settings.model ?? '');
            }}
            disabled={disabled}
          >
            Other model…
          </button>
          {customVisible && (
            <form
              className="custom-model"
              onSubmit={(e) => {
                e.preventDefault();
                closeAfter(settings({ model: custom.trim() }));
              }}
            >
              <input
                aria-label="Custom model ID"
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
                autoFocus
                placeholder="Model ID"
              />
              <button disabled={disabled || !custom.trim()}>Use model</button>
            </form>
          )}
          <div className="menu-heading">Reasoning</div>
          <div className="reasoning-options" role="group" aria-label="Reasoning effort">
            {(model?.efforts ?? (chat.settings.effort ? [chat.settings.effort] : [])).map((effort) => (
              <button
                key={effort}
                aria-pressed={chat.settings.effort === effort}
                disabled={disabled}
                onClick={() => closeAfter(settings({ effort }))}
              >
                {effort}
              </button>
            ))}
            {!model && !chat.settings.effort && <small>Unavailable for this model</small>}
          </div>
        </>,
      )}
      {options(
        'permissions',
        permission,
        <div role="group" aria-label="Permissions">
          {(['Ask for approval', 'Auto-review', 'Full Access'] as const).map((label) => (
            <button
              key={label}
              className="composer-choice"
              aria-pressed={permission === label}
              disabled={disabled}
              onClick={() => {
                if (label === 'Full Access') {
                  setMenu(null);
                  setConfirmFull(true);
                } else
                  closeAfter(
                    settings({
                      permissions: 'workspace',
                      reviewer: label === 'Ask for approval' ? 'user' : 'auto_review',
                    }),
                  );
              }}
            >
              <span>
                <strong>{label}</strong>
                <small>
                  {label === 'Ask for approval'
                    ? 'You approve restricted actions'
                    : label === 'Auto-review'
                      ? 'Codex reviews approval requests'
                      : 'Run with the remote user’s full permissions'}
                </small>
              </span>
              {permission === label && <Check size={14} />}
            </button>
          ))}
        </div>,
        <ShieldCheck size={14} />,
      )}
      {confirmFull && (
        <Modal
          title="Enable Full Access?"
          description={`Codex can run commands with the remote user's permissions on ${chat.server}.`}
          onClose={() => setConfirmFull(false)}
        >
          <div className="actions">
            <button onClick={() => setConfirmFull(false)}>Cancel</button>
            <button
              className="primary"
              disabled={disabled}
              onClick={() => {
                void settings({ permissions: 'full_access' })
                  .then(() => setConfirmFull(false))
                  .catch(() => {});
              }}
            >
              Enable Full Access
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
