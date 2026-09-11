if (import.meta.env.MODE === 'e2e') {
  await import('@wdio/tauri-plugin');
}
import { createRoot } from 'react-dom/client';
import App from './app/App';
import './styles/chat.css';
import './styles/composer.css';
import './styles/controls.css';
import './styles/editor.css';
import './styles/files.css';
import './styles/goals.css';
import './styles/navigation.css';
import './styles/session-status.css';
import './styles/settings.css';
import './styles/theme.css';
import './styles/usage.css';
import { installScrollbars } from './ui/scroll/controller';
import './ui/scroll/scroll.css';
const disposeScrollbars = installScrollbars(document);
import.meta.hot?.dispose(disposeScrollbars);
const root = document.getElementById('root');
if (root) {
  const renderer = createRoot(root);
  renderer.render(<App />);
  if (import.meta.env.MODE === 'e2e') {
    (await import('../tests/performance.fixture')).installPerformanceFixture(renderer);
    (await import('../tests/controls.fixture')).installControlsFixture(renderer);
  }
}
