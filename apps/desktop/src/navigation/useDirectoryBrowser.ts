import { useEffect, useRef } from 'react';
import { DirectoryBrowser } from './directoryBrowser';
export function useDirectoryBrowser(server: string, report: (error: unknown) => void) {
  const browser = useRef<DirectoryBrowser | null>(null);
  const reporter = useRef(report);
  reporter.current = report;
  useEffect(() => {
    const lease = new DirectoryBrowser(server, (error) => reporter.current(error));
    browser.current = lease;
    return () => {
      browser.current = null;
      lease.dispose();
    };
  }, [server]);
  return {
    list: (path: string) => browser.current!.list(path),
    create: (parent: string, name: string) => browser.current!.create(parent, name),
  };
}
