/**
 * Calls `fetch` now and then every `intervalMs`, handing each answer to `onValue`. Returns a
 * function that stops it; an answer that arrives after stopping is dropped. A run never has two
 * requests in flight, and the "in flight" state belongs to the run: React starts, stops and
 * restarts effects in development, and a flag shared between runs would let the stopped run's
 * request block the live one until the next tick.
 */
export function startPolling<T>(
  fetch: () => Promise<T>,
  onValue: (value: T) => void,
  onError: (error: unknown) => void,
  intervalMs: number,
): () => void {
  let stopped = false;
  let inFlight = false;
  const poll = () => {
    if (inFlight) return;
    inFlight = true;
    fetch().then(
      (value) => { if (!stopped) onValue(value); },
      (error: unknown) => { if (!stopped) onError(error); },
    ).finally(() => { inFlight = false; });
  };
  poll();
  const timer = setInterval(poll, intervalMs);
  return () => { stopped = true; clearInterval(timer); };
}
