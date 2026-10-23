import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { startPolling } from "./poller";

const settle = () => new Promise((resolve) => { queueMicrotask(() => { queueMicrotask(() => { resolve(undefined); }); }); });

describe("polling", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it("asks at once, without waiting for the first tick", async () => {
    const onValue = vi.fn();
    const stop = startPolling(() => Promise.resolve(1), onValue, vi.fn(), 3000);
    await settle();
    expect(onValue).toHaveBeenCalledWith(1);
    stop();
  });

  it("shows the answer right away when started, stopped and started again, as React does in development", async () => {
    const answer = vi.fn(() => Promise.resolve("folders"));
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = startPolling(answer, first, vi.fn(), 3000);
    stopFirst();
    const stopSecond = startPolling(answer, second, vi.fn(), 3000);
    await settle();
    expect(answer).toHaveBeenCalledTimes(2);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith("folders");
    stopSecond();
  });

  it("never has two requests in flight and keeps going on schedule", async () => {
    let release: (value: number) => void = () => undefined;
    const slow = vi.fn(() => new Promise<number>((resolve) => { release = resolve; }));
    const onValue = vi.fn();
    const stop = startPolling(slow, onValue, vi.fn(), 1000);
    await vi.advanceTimersByTimeAsync(3500);
    expect(slow).toHaveBeenCalledTimes(1);
    release(7);
    await settle();
    expect(onValue).toHaveBeenCalledWith(7);
    await vi.advanceTimersByTimeAsync(1000);
    expect(slow).toHaveBeenCalledTimes(2);
    stop();
  });

  it("reports errors, keeps polling, and drops answers that arrive after stopping", async () => {
    const onError = vi.fn();
    let calls = 0;
    const flaky = vi.fn(() => (++calls === 1 ? Promise.reject(new Error("no")) : Promise.resolve(calls)));
    const onValue = vi.fn();
    const stop = startPolling(flaky, onValue, onError, 1000);
    await settle();
    expect(onError).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(onValue).toHaveBeenCalledWith(2);
    let late: (value: number) => void = () => undefined;
    const hanging = vi.fn(() => new Promise<number>((resolve) => { late = resolve; }));
    const lateValue = vi.fn();
    const stopLate = startPolling(hanging, lateValue, vi.fn(), 1000);
    stopLate();
    late(9);
    await settle();
    expect(lateValue).not.toHaveBeenCalled();
    stop();
  });
});
