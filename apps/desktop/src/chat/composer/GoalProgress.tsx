import { MoreHorizontal, Pause, Play, Target } from 'lucide-react';
import { Popover } from 'radix-ui';
import { useEffect, useState } from 'react';
import type { SessionAction } from '../../bridge/session';
import type { Goal } from '../../bridge/types';
import { IconButton } from '../../ui/controls';
import { duration } from '../time';
export const goalLabels: Record<Goal['status'], string> = {
  active: 'Working toward goal',
  paused: 'Paused',
  blocked: 'Needs attention',
  usage_limited: 'Usage limit reached',
  budget_limited: 'Token budget reached',
  complete: 'Complete',
};
export function GoalProgress({
  goal,
  running,
  busy,
  ready,
  perform,
  onNewGoal,
}: {
  goal: Goal;
  running: boolean;
  busy: boolean;
  ready: boolean;
  perform: (action: SessionAction) => Promise<void>;
  onNewGoal: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [pausing, setPausing] = useState(false);
  useEffect(() => {
    if (!running) setPausing(false);
  }, [running]);
  const [editing, setEditing] = useState(false);
  const [objective, setObjective] = useState(goal.objective);
  const [budget, setBudget] = useState(goal.token_budget?.toString() ?? '');
  const control = (action: 'pause' | 'resume' | 'clear') => {
    if (action === 'pause') setPausing(true);
    void perform({ action: 'goal', goal: { action } })
      .then(() => setOpen(false))
      .catch(() => setPausing(false));
  };
  const paused = goal.status === 'paused' && running && pausing;
  return (
    <div className="goal-progress" aria-label="Session goal">
      <Target size={14} />
      <span className="goal-objective" title={goal.objective}>
        {goal.objective}
      </span>
      <small
        title={`${goal.tokens_used.toLocaleString()}${goal.token_budget === null ? ' tokens used' : ` / ${goal.token_budget.toLocaleString()} tokens`}`}
      >
        {paused ? 'Pausing after this turn' : goalLabels[goal.status]} · {duration(goal.time_used_seconds)}
      </small>
      {goal.status === 'active' ? (
        <IconButton label="Pause after this turn" disabled={busy} onClick={() => control('pause')}>
          <Pause size={14} />
        </IconButton>
      ) : (
        goal.status !== 'complete' && (
          <IconButton label="Resume goal" disabled={busy || running || !ready} onClick={() => control('resume')}>
            <Play size={14} />
          </IconButton>
        )
      )}
      <Popover.Root
        open={open}
        onOpenChange={(value) => {
          setOpen(value);
          setEditing(false);
          setObjective(goal.objective);
          setBudget(goal.token_budget?.toString() ?? '');
        }}
      >
        <Popover.Trigger asChild>
          <button className="icon-button" aria-label="Goal options">
            <MoreHorizontal size={16} />
          </button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            className="composer-menu goal-menu"
            side="top"
            align="end"
            sideOffset={8}
            aria-label="Goal options"
          >
            {editing ? (
              <form
                onSubmit={(event) => {
                  event.preventDefault();
                  const tokenBudget = budget ? Number(budget) : null;
                  const action: SessionAction = {
                    action: 'goal',
                    goal:
                      objective.trim() === goal.objective
                        ? { action: 'budget', token_budget: tokenBudget }
                        : { action: 'set', objective: objective.trim(), token_budget: tokenBudget },
                  };
                  void perform(action)
                    .then(() => setOpen(false))
                    .catch(() => {});
                }}
              >
                <label className="field">
                  <span>Goal</span>
                  <textarea
                    autoFocus
                    rows={3}
                    maxLength={4000}
                    value={objective}
                    onChange={(e) => setObjective(e.target.value)}
                    required
                  />
                </label>
                <label className="field">
                  <span>
                    Token budget <small>Optional</small>
                  </span>
                  <input
                    type="number"
                    min={1}
                    step={1}
                    max={Number.MAX_SAFE_INTEGER}
                    value={budget}
                    onChange={(e) => setBudget(e.target.value)}
                    placeholder="No token budget"
                  />
                </label>
                {objective.trim() !== goal.objective && <small>Changing the goal resets its usage tracking.</small>}
                <div className="actions">
                  <button type="button" onClick={() => setEditing(false)}>
                    Back
                  </button>
                  <button disabled={busy || running || !ready || !objective.trim()}>Save changes</button>
                </div>
              </form>
            ) : (
              <>
                <p className="goal-detail">{goal.objective}</p>
                <small>
                  {goal.tokens_used.toLocaleString()}
                  {goal.token_budget === null ? ' tokens used' : ` / ${goal.token_budget.toLocaleString()} tokens`}
                </small>
                <button
                  className="composer-choice"
                  disabled={busy || running || !ready}
                  onClick={() => {
                    if (goal.status === 'complete')
                      void onNewGoal()
                        .then(() => setOpen(false))
                        .catch(() => {});
                    else setEditing(true);
                  }}
                >
                  {goal.status === 'complete' ? 'Start a new goal…' : 'Edit goal and budget…'}
                </button>
                <button className="composer-choice" disabled={busy} onClick={() => control('clear')}>
                  Clear goal
                </button>
              </>
            )}
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </div>
  );
}
