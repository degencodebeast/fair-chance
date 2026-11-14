// Re-reads a screen every few seconds, never while the tab is hidden, and slows down when the
// FastNEAR API or the RPC says 429 or answers 5xx. The timers are given in, so tests use fake ones.
export const REFRESH_MS = 4_000;
const BACKOFF_MS = [4_000, 8_000, 16_000, 32_000, 60_000] as const;

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
  return Math.max(step, retryAfterMs ?? 0);
}

function shouldBackOff(e: unknown): e is HttpStatusError {
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
      if (shouldBackOff(e)) {
        failures += 1;
        delay = backoffDelayMs(failures, e.retryAfterMs);
      }
    } finally {
      running = false;
    }
    schedule(delay);
  }

  return {
    start() {
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

/** Starts a refresher tied to the page: it pauses while the tab is hidden. The result stops it. */
export function refreshWhileShown(run: () => Promise<void>, doc: Document = document): () => void {
  const refresher = createRefresher({ run, isHidden: () => doc.hidden });
  const onChange = (): void => refresher.visibilityChanged();
  doc.addEventListener("visibilitychange", onChange);
  refresher.start();
  return () => {
    refresher.stop();
    doc.removeEventListener("visibilitychange", onChange);
  };
}
