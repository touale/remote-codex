import { call, failure, operationId } from '../bridge/client';
import type { DirectoryPage } from '../bridge/types';
type Invoke = typeof call;

/** One lease per dialog. Late open responses are released even after unmount. */
export class DirectoryBrowser {
  private id: string | null = null;
  private opening: Promise<string> | null = null;
  private disposed = false;
  private operations = new Set<string>();
  constructor(
    private server: string,
    private report: (error: unknown) => void,
    private invoke: Invoke = call,
  ) {}
  private async request<T>(run: (operation: string) => Promise<T>): Promise<T> {
    if (this.disposed) throw { code: 'OPERATION_CANCELLED', message: 'Directory browser is closed.' };
    const operation = operationId();
    this.operations.add(operation);
    try {
      return await run(operation);
    } finally {
      this.operations.delete(operation);
    }
  }
  private open(): Promise<string> {
    if (this.id) return Promise.resolve(this.id);
    if (!this.opening) {
      this.opening = this.request((operationId) => this.invoke('directory_open', { server: this.server, operationId }))
        .then((id) => {
          if (this.disposed) {
            this.release(id);
            throw { code: 'OPERATION_CANCELLED', message: 'Directory browser is closed.' };
          }
          this.id = id;
          return id;
        })
        .finally(() => {
          this.opening = null;
        });
    }
    return this.opening;
  }
  async list(path: string): Promise<DirectoryPage> {
    const browser = await this.open();
    return this.request((operationId) => this.invoke('directory_browse', { browser, path, operationId }));
  }
  async create(parent: string, name: string): Promise<string> {
    const browser = await this.open();
    return this.request((operationId) => this.invoke('directory_create', { browser, parent, name, operationId }));
  }
  private release(id: string) {
    void this.invoke('directory_close', { browser: id }).catch((error) => {
      if (failure(error).code !== 'RESOURCE_UNAVAILABLE') this.report(error);
    });
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    for (const id of this.operations) void this.invoke('cancel_operation', { id }).catch(this.report);
    if (this.id) {
      this.release(this.id);
      this.id = null;
    }
  }
}
