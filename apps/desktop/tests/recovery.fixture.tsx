import { useRef, useState } from 'react';
import { Composer } from '../src/chat/composer/Composer';
import type { SessionAction } from '../src/bridge/session';

export function RecoveryFixture() {
  const [draft, setDraft] = useState('Continue with this new instruction.');
  const [connection, setConnection] = useState<'failed' | 'connecting' | 'ready' | 'blocked'>('failed');
  const [sent, setSent] = useState(0);
  const pending = useRef<{ resolve: () => void; reject: (error: Error) => void } | null>(null);
  const finish = (success: boolean) => {
    setConnection(success ? 'ready' : 'failed');
    if (success) pending.current?.resolve();
    else pending.current?.reject(new Error('This message was not submitted.'));
    pending.current = null;
  };
  const onAction = async (action: SessionAction) => {
    if (action.action === 'interrupt') return finish(false);
    if (action.action !== 'submit' && action.action !== 'settings') throw new Error('Unexpected action');
    setConnection('connecting');
    await new Promise<void>((resolve, reject) => {
      pending.current = { resolve, reject };
    });
    if (action.action === 'submit') {
      setSent((value) => value + 1);
      setDraft('');
    }
  };
  return (
    <div>
      <button onClick={() => finish(false)}>Fail reconnect</button>
      <button onClick={() => finish(true)}>Restore connection</button>
      <button onClick={() => setConnection('blocked')}>Block identity</button>
      <output id="recovery-submitted">{sent}</output>
      <Composer
        chat={{
          server: 'fixture',
          draft,
          composerMode: 'code',
          goal: null,
          turn: null,
          ready: connection === 'ready',
          canReconnect: connection === 'failed',
          models: [],
          settings: { mode: 'agent', model: 'fixture-model', effort: 'low', full_access: false, reviewer: 'user' },
        }}
        setDraft={setDraft}
        setComposerMode={() => {}}
        onAction={onAction}
        onStatus={() => {}}
      />
    </div>
  );
}
