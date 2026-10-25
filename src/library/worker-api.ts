import { MAX_WINDOW, type LibraryApi, type LibraryPage, type LibraryQuery, type WorkerReply, type WorkerRequest } from "./types";

interface Pending {
  resolve: (reply: WorkerReply) => void;
  reject: (error: Error) => void;
  removeAbort: () => void;
}

export class WorkerLibraryApi implements LibraryApi {
  startupError?: string;
  private worker = new Worker(new URL("./catalog.worker.ts", import.meta.url), { type: "module" });
  private pending = new Map<number, Pending>();
  private nextId = 0;
  private failure: Error | null = null;

  constructor() {
    this.worker.onmessage = ({ data }: MessageEvent<WorkerReply>) => {
      const request = this.pending.get(data.id);
      if (!request) return;
      this.pending.delete(data.id);
      request.removeAbort();
      if ("error" in data) request.reject(new Error(data.error));
      else request.resolve(data);
    };
    this.worker.onerror = () => this.fail(new Error("The demo library worker stopped. Reload the app to retry."));
    this.worker.onmessageerror = () => this.fail(new Error("The demo library returned unreadable data. Reload the app."));
  }

  read(query: LibraryQuery, start: number, count: number, signal: AbortSignal): Promise<LibraryPage> {
    if (!Number.isInteger(start) || start < 0 || !Number.isInteger(count) || count < 1 || count > MAX_WINDOW) {
      return Promise.reject(new Error(`Request a non-negative start and 1-${MAX_WINDOW} rows.`));
    }
    return this.ask((id) => ({ id, query, start, count }), signal).then((reply) => {
      if ("page" in reply) return reply.page;
      throw new Error("The demo library returned unreadable data. Reload the app.");
    });
  }

  ids(query: LibraryQuery): Promise<number[]> {
    return this.ask((id) => ({ id, query, ids: true }), null).then((reply) => {
      if ("ids" in reply) return reply.ids;
      throw new Error("The demo library returned unreadable data. Reload the app.");
    });
  }

  /** Send one request to the worker; `signal`, when given, can cancel it. */
  private ask(request: (id: number) => WorkerRequest, signal: AbortSignal | null): Promise<WorkerReply> {
    if (this.failure) return Promise.reject(this.failure);
    if (signal?.aborted) return Promise.reject(new DOMException("Request cancelled", "AbortError"));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const abort = () => {
        this.pending.delete(id);
        reject(new DOMException("Request cancelled", "AbortError"));
      };
      signal?.addEventListener("abort", abort, { once: true });
      const removeAbort = () => signal?.removeEventListener("abort", abort);
      this.pending.set(id, { resolve, reject, removeAbort });
      try {
        this.worker.postMessage(request(id));
      } catch (error) {
        this.pending.delete(id);
        removeAbort();
        reject(error instanceof Error ? error : new Error("Could not request demo library data."));
      }
    });
  }

  dispose() {
    this.fail(new Error("The demo library was closed."));
  }

  private fail(error: Error) {
    this.failure = error;
    this.worker.terminate();
    for (const request of this.pending.values()) {
      request.removeAbort();
      request.reject(error);
    }
    this.pending.clear();
  }
}
