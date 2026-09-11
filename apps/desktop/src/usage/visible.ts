import { useEffect, useRef } from 'react';
export function useVisibleRefresh(refresh: () => void, enabled: boolean) {
  const current = useRef(refresh);
  current.current = refresh;
  useEffect(() => {
    if (!enabled) return;
    const update = () => {
      if (!document.hidden) current.current();
    };
    update();
    const timer = setInterval(update, 60000);
    document.addEventListener('visibilitychange', update);
    window.addEventListener('focus', update);
    return () => {
      clearInterval(timer);
      document.removeEventListener('visibilitychange', update);
      window.removeEventListener('focus', update);
    };
  }, [enabled]);
}
