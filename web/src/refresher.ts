// Re-reads a screen every few seconds, never while the tab is hidden, and slows down when the
// FastNEAR API or the RPC says 429 or answers 5xx. The timers are given in, so tests use fake ones.
export const REFRESH_MS = 4_000;
const BACKOFF_MS = [4_000, 8_000, 16_000, 32_000, 60_000] as const;
const MAX_DELAY_MS = 60_000;

/** A read that failed with an HTTP status; `retryAfterMs` comes from the Retry-After header. */
export class HttpStatusError extends Error {
  status: number;
  retryAfterMs: number | null;
  constructor(message: string, status: number, retryAfterMs: number | null) {
    super(message);
    this.status = status;
    this.retryAfterMs = retryAfterMs;
  }
}

export function httpStatusError(message: string, resp: Response): HttpStatusError {
  const header = resp.headers.get("Retry-After");
  const seconds = header !== null && /^\d+$/.test(header.trim()) ? Number(header.trim()) : null;
  return new HttpStatusError(message, resp.status, seconds === null ? null : seconds * 1000);
}

/** How long to wait after the nth failure in a row (1 or more). */
export function backoffDelayMs(failures: number, retryAfterMs: number | null): number {
  const step = BACKOFF_MS[Math.min(failures, BACKOFF_MS.length) - 1]!;
  // Retry-After may lengthen a step, never past the last step. A huge value would also overflow setTimeout.
  return Math.min(MAX_DELAY_MS, Math.max(step, retryAfterMs ?? 0));
}

/** True for a 429 or a 5xx answer: the read may run again after a backoff. */
export function shouldBackOff(e: unknown): e is HttpStatusError {
  return e instanceof HttpStatusError && (e.status === 429 || e.status >= 500);
}

export interface RefresherTimers {
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(id: unknown): void;
}

export interface RefresherOptions {
  /** One read and redraw. It throws when the read fails. */
  run: () => Promise<void>;
  isHidden: () => boolean;
  /** Called after each failed read. `retrying` is false when the loop has stopped for good. */
  onError?: (error: unknown, retrying: boolean) => void;
  timers?: RefresherTimers;
  intervalMs?: number;
}

export interface Refresher {
  start(): void;
  stop(): void;
  /** Call when the tab is shown or hidden. */
  visibilityChanged(): void;
}

export function createRefresher(options: RefresherOptions): Refresher {
  const timers: RefresherTimers = options.timers ?? { setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>) };
  const interval = options.intervalMs ?? REFRESH_MS;
  let timer: unknown = null;
  let running = false;
  let stopped = true;
  let failures = 0;

  function clear(): void {
    if (timer !== null) timers.clearTimeout(timer);
    timer = null;
  }

  function schedule(ms: number): void {
    clear();
    if (stopped || options.isHidden()) return;
    timer = timers.setTimeout(() => void tick(), ms);
  }

  async function tick(): Promise<void> {
    timer = null;
    if (stopped || options.isHidden() || running) return;
    running = true;
    let delay = interval;
    try {
      await options.run();
      failures = 0;
    } catch (e) {
      // Only a rate limit or a server error is worth another try. A 403 or an answer this page does not
      // know ends the loop; the reader sees the error and no timer stays.
      const retrying = shouldBackOff(e);
      options.onError?.(e, retrying);
      if (!retrying) {
        running = false;
        stopped = true;
        return;
      }
      failures += 1;
      delay = backoffDelayMs(failures, e.retryAfterMs);
    } finally {
      running = false;
    }
    schedule(delay);
  }

  return {
    start() {
      clear();
      stopped = false;
      failures = 0;
      schedule(interval);
    },
    stop() {
      stopped = true;
      clear();
    },
    visibilityChanged() {
      if (stopped) return;
      if (options.isHidden()) clear();
      else if (!running) schedule(0);
    },
  };
}

/** A refresher tied to the page: it pauses while the tab is hidden. `restart` begins again after a stop. */
export function refreshWhileShown(run: () => Promise<void>, onError?: RefresherOptions["onError"], doc: Document = document): { stop: () => void; restart: () => void } {
  const refresher = createRefresher({ run, isHidden: () => doc.hidden, ...(onError ? { onError } : {}) });
  const onChange = (): void => refresher.visibilityChanged();
  const handle = {
    stop: () => {
      refresher.stop();
      doc.removeEventListener("visibilitychange", onChange);
    },
    restart: () => {
      doc.removeEventListener("visibilitychange", onChange);
      doc.addEventListener("visibilitychange", onChange);
      refresher.start();
    },
  };
  handle.restart();
  return handle;
}
