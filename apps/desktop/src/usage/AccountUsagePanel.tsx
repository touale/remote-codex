import { RotateCw } from 'lucide-react';
import { IconButton } from '../ui/controls';
import { Limits } from './Limits';
import { refreshNativeUsage, useNativeUsage } from './native';
export function AccountUsagePanel() {
  const state = useNativeUsage(true);
  return (
    <section className="settings-group account-usage">
      <div className="status-heading">
        <h3>Usage limits</h3>
        <IconButton label="Refresh usage limits" disabled={state.loading} onClick={() => void refreshNativeUsage(true)}>
          <RotateCw size={14} className={state.loading ? 'spinning' : ''} />
        </IconButton>
      </div>
      <Limits
        limits={state.value?.limits ?? []}
        updatedAt={state.updatedAt}
        error={state.error}
        loading={state.loading}
        unavailable={
          state.value?.availability === 'signed_out'
            ? 'Sign in with ChatGPT to view your usage limits.'
            : state.value?.availability === 'unsupported'
              ? 'This account or provider does not report subscription usage limits.'
              : undefined
        }
      />
    </section>
  );
}
