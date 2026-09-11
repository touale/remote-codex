import { Copy, RotateCw } from 'lucide-react';
import type { ChatState, ChatUpdate } from './state';

import { IconButton, Modal } from '../ui/controls';
import { Limits } from '../usage/Limits';
import { useSessionUsage } from '../usage/session';
import { goalLabels } from './composer/GoalProgress';
import { duration } from './time';
export function SessionStatus({
  chat,
  update,
  onClose,
  report,
}: {
  chat: ChatState;
  update: ChatUpdate;
  onClose: () => void;
  report: (error: unknown) => void;
}) {
  const { loading, refresh } = useSessionUsage(chat, update, true);
  const { status } = chat;
  const usage = status.usage;
  const activity = chat.closed
    ? 'Closed'
    : chat.questions.length
      ? 'Waiting for your input'
      : chat.turn
        ? 'Working'
        : ({ idle: 'Idle', active: 'Working', notLoaded: 'Not loaded', systemError: 'Error' }[status.activity] ??
          status.activity);
  const environment = {
    ready: 'Ready',
    reconnecting: 'Reconnecting',
    recovering: 'Recovering',
    action_required: 'Needs attention',
    closed: 'Closed',
  }[chat.environment.status];
  const row = (name: string, value: React.ReactNode) => (
    <div className="status-row">
      <dt>{name}</dt>
      <dd>{value}</dd>
    </div>
  );
  return (
    <Modal
      title="Session status"
      onClose={onClose}
      description="Current settings, context and usage for this conversation."
    >
      <div className="session-status">
        <dl>
          {row(
            'Session',
            <span className="status-id">
              <code>{chat.session.id}</code>
              <IconButton
                label="Copy session ID"
                onClick={() => {
                  void navigator.clipboard.writeText(chat.session.id).catch(report);
                }}
              >
                <Copy size={13} />
              </IconButton>
            </span>,
          )}
          {row('Workspace', `${chat.server} · ${chat.session.cwd}`)}
          {row('Model', `${chat.settings.model}${chat.settings.effort ? ` · ${chat.settings.effort}` : ''}`)}
          {row('Provider', status.provider ?? 'Unavailable')}
          {row('Mode', chat.composerMode === 'goal' ? 'Goal' : chat.settings.mode === 'plan' ? 'Plan' : 'Code')}
          {row(
            'Permissions',
            chat.settings.full_access
              ? 'Full Access'
              : chat.settings.reviewer === 'user'
                ? 'Ask for approval'
                : 'Auto-review',
          )}
          {row('Codex', activity)}
          {status.active_flags.length > 0 &&
            row('Activity', status.active_flags.map((v) => v.replace(/([A-Z])/g, ' $1')).join(', '))}
          {row('Execution environment', environment)}
        </dl>
        <div className="status-section">
          <h3>Context</h3>
          {usage ? (
            <>
              <dl>
                {row('Latest turn', `${usage.last_tokens.toLocaleString()} tokens`)}
                {row(
                  'Context window',
                  usage.context_window == null ? 'Unavailable' : `${usage.context_window.toLocaleString()} tokens`,
                )}
                {row(
                  'Input / cached',
                  `${usage.input_tokens.toLocaleString()} / ${usage.cached_input_tokens.toLocaleString()}`,
                )}
                {row(
                  'Output / reasoning',
                  `${usage.output_tokens.toLocaleString()} / ${usage.reasoning_tokens.toLocaleString()}`,
                )}
                {row('Session total', `${usage.total_tokens.toLocaleString()} tokens`)}
              </dl>
              <small>Latest turn usage is separate from the session’s cumulative total.</small>
            </>
          ) : (
            <p className="muted">Context usage is unavailable until Codex reports it.</p>
          )}
        </div>
        <div className="status-section">
          <div className="status-heading">
            <h3>Usage limits</h3>
            <IconButton
              label="Refresh session status"
              disabled={loading || chat.closed}
              onClick={() => void refresh(true)}
            >
              <RotateCw size={13} className={loading ? 'spinning' : ''} />
            </IconButton>
          </div>
          <Limits
            limits={status.limits}
            updatedAt={status.limits_updated_at}
            error={status.limits_error}
            loading={loading}
          />
        </div>
        {chat.goal && (
          <div className="status-section">
            <h3>Goal · {goalLabels[chat.goal.status]}</h3>
            <p>{chat.goal.objective}</p>
            <small>
              {duration(chat.goal.time_used_seconds)} · {chat.goal.tokens_used.toLocaleString()}
              {chat.goal.token_budget == null ? ' tokens used' : ` / ${chat.goal.token_budget.toLocaleString()} tokens`}
            </small>
          </div>
        )}
        {chat.closed && <p className="muted">This session is closed. Values reflect its last available state.</p>}
      </div>
    </Modal>
  );
}
