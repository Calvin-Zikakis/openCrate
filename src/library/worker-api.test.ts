import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkerLibraryApi } from "./worker-api";
import { DEFAULT_QUERY, type WorkerReply } from "./types";

class FakeWorker {
  static current: FakeWorker;
  onmessage: ((event: { data: WorkerReply }) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessageerror: (() => void) | null = null;
  postMessage = vi.fn();
  terminate = vi.fn();
  constructor() { FakeWorker.current = this; }
  reply(data: WorkerReply) { this.onmessage?.({ data }); }
}
beforeEach(() => vi.stubGlobal("Worker", FakeWorker));
afterEach(() => vi.unstubAllGlobals());

describe("worker requests", () => {
  it("correlates out-of-order replies and removes cancelled requests", async () => {
    const api = new WorkerLibraryApi();
    const abort = new AbortController();
    const first = api.read(DEFAULT_QUERY, 0, 20, abort.signal);
    const firstRejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = api.read({ ...DEFAULT_QUERY, search: "new" }, 0, 20, new AbortController().signal);
    abort.abort();
    FakeWorker.current.reply({ id: 1, page: { start: 0, total: 100, items: [] } });
    FakeWorker.current.reply({ id: 2, page: { start: 0, total: 2, items: [] } });
    await firstRejected;
    expect((await second).total).toBe(2);
    api.dispose();
  });
  it("surfaces worker failures and rejects future requests", async () => {
    const api = new WorkerLibraryApi();
    const request = api.read(DEFAULT_QUERY, 0, 20, new AbortController().signal);
    const rejected = expect(request).rejects.toThrow("worker stopped");
    FakeWorker.current.onerror?.();
    await rejected;
    await expect(api.read(DEFAULT_QUERY, 0, 20, new AbortController().signal)).rejects.toThrow("worker stopped");
    expect(FakeWorker.current.terminate).toHaveBeenCalledOnce();
  });
  it("rejects invalid windows and already cancelled work without posting", async () => {
    const api = new WorkerLibraryApi();
    await expect(api.read(DEFAULT_QUERY, 0, 201, new AbortController().signal)).rejects.toThrow("1-200");
    const controller = new AbortController();
    controller.abort();
    await expect(api.read(DEFAULT_QUERY, 0, 20, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(FakeWorker.current.postMessage).not.toHaveBeenCalled();
    api.dispose();
  });
});
