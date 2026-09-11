import { Monitor, Moon, Sun } from 'lucide-react';
import { Switch } from 'radix-ui';
import type { AppPreferences } from '../bridge/preferences';
import { changeAppPreferences, useAppPreferences } from './preferences';

const details = [
  ['five_hour_limit', '5-hour limit', 'Remaining allowance in your current 5-hour window.'],
  ['weekly_limit', 'Weekly limit', 'Remaining allowance in your weekly window.'],
  ['context_usage', 'Context usage', 'How much of this conversation’s context is in use.'],
  ['session_tokens', 'Session tokens', 'Total tokens reported for this conversation.'],
] as const;
export function GeneralSettings({ report }: { report: (error: unknown) => void }) {
  const preferences = useAppPreferences();
  return (
    <div className="general-settings">
      <section className="settings-group">
        <h3>Appearance</h3>
        <div className="setting-row">
          <div>
            <strong>Theme</strong>
            <small>Choose how Remote Codex looks.</small>
          </div>
          <div className="theme-options" role="group" aria-label="Appearance">
            {(['system', 'light', 'dark'] as const).map((theme) => {
              const Icon = { system: Monitor, light: Sun, dark: Moon }[theme];
              return (
                <button
                  key={theme}
                  aria-pressed={preferences.theme === theme}
                  onClick={() => void changeAppPreferences({ theme }).catch(report)}
                >
                  <Icon size={14} />
                  {theme[0].toUpperCase() + theme.slice(1)}
                </button>
              );
            })}
          </div>
        </div>
      </section>
      <section className="settings-group">
        <h3>Conversation details</h3>
        <p className="settings-caption">Choose what appears below the message box.</p>
        {details.map(([key, label, description]) => (
          <div className="setting-row" key={key}>
            <label htmlFor={`setting-${key}`}>
              <strong>{label}</strong>
              <small>{description}</small>
            </label>
            <Switch.Root
              id={`setting-${key}`}
              className="setting-switch"
              checked={preferences[key]}
              onCheckedChange={(checked) =>
                void changeAppPreferences({ [key]: checked } as Partial<AppPreferences>).catch(report)
              }
            >
              <Switch.Thumb className="switch-thumb" />
            </Switch.Root>
          </div>
        ))}
      </section>
      <p className="settings-caption">Applies to all windows. Changes are saved automatically.</p>
    </div>
  );
}
