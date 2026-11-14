// How the page sends one chain read: through a shared limit of reads in flight, with a short retry
// when the read got no readable answer, a 429 or a 5xx. Only reads come here. A send to the wallet
// and anything that signs never do, so nothing is ever sent twice.
import { HttpStatusError, shouldBackOff } from "./refresher.ts";

/** Chain reads in flight at once for the whole page, over the RPC and the FastNEAR transactions API. */
export const READS_AT_ONCE = 4;
const RETRY_STEPS_MS = [1_000, 2_000, 4_000] as const;
const MAX_RETRY_AFTER_MS = 8_000;

export interface ReadLimiter {
  run<T>(read: () => Promise<T>): Promise<T>;
}

/** Runs at most `max` reads at a time; the others wait in order. */
export function createReadLimiter(max: number): ReadLimiter {
  let active = 0;
  const waiting: Array<() => void> = [];
  return {
    async run<T>(read: () => Promise<T>): Promise<T> {
      if (active >= max) await new Promise<void>((r) => waiting.push(r));
      else active++;
      try {
        return await read();
      } finally {
        // The slot passes straight to the next read in line, so `active` stays the same.
        const next = waiting.shift();
        if (next) next();
        else active--;
      }
    },
  };
}

/** The one limit every chain read of the page shares. */
export const pageReads = createReadLimiter(READS_AT_ONCE);

/** A read that may work a moment later: no readable answer, a 429 or a 5xx. */
export function isRetryableRead(e: unknown): boolean {
  return shouldBackOff(e);
}

/** The wait before retry number `n` (1 to 3). A Retry-After may lengthen it, never past 8 seconds. */
export function retryDelayMs(n: number, retryAfterMs: number | null): number {
  const step = RETRY_STEPS_MS[Math.min(n, RETRY_STEPS_MS.length) - 1]!;
  return Math.max(step, Math.min(MAX_RETRY_AFTER_MS, retryAfterMs ?? 0));
}

export interface RetryOptions {
  limiter?: ReadLimiter;
  sleep?: (ms: number) => Promise<void>;
}

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * One read, tried up to 3 more times on a retryable failure. Each try takes a slot of the limiter;
 * the wait between tries holds none. The last error is thrown unchanged, so the texts built on it stay.
 */
export async function readWithRetry<T>(read: () => Promise<T>, options: RetryOptions = {}): Promise<T> {
  const limiter = options.limiter ?? pageReads;
  const sleep = options.sleep ?? wait;
  for (let retry = 1; ; retry++) {
    try {
      return await limiter.run(read);
    } catch (e) {
      if (retry > RETRY_STEPS_MS.length || !isRetryableRead(e)) throw e;
      await sleep(retryDelayMs(retry, e instanceof HttpStatusError ? e.retryAfterMs : null));
    }
  }
}
