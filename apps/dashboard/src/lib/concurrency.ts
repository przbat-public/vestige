interface BulkOutcome<T, R> {
  /** Items that resolved successfully, with their result. */
  succeeded: { item: T; value: R }[];
  /** Items that rejected, with the error thrown. */
  failed: { item: T; error: unknown }[];
}

/**
 * Run an async fn over each item with bounded parallelism, never throwing.
 *
 * Why not `Promise.all`/`Promise.allSettled` directly:
 *   - `Promise.all` aborts on first rejection — bad for bulk UI flows where
 *     you want to apply as many operations as possible and report partial
 *     failure to the user.
 *   - `Promise.allSettled` runs everything in parallel; with 100 selected
 *     items on a slow network that's 100 in-flight requests. Most browsers
 *     queue beyond 6, but the backend rate-limiter still sees the load.
 *
 * Concurrency defaults to 5 (matches typical browser HTTP/1.1 connection
 * limit; HTTP/2 callers can pass higher). Order of returned successes is
 * input order; order of failures matches the order in which they reject.
 *
 * Cancellation is intentionally not supported — if you need it, wire an
 * AbortController into `fn` and ignore the resolved results.
 */
export async function runWithConcurrency<T, R>(
  items: readonly T[],
  fn: (item: T) => Promise<R>,
  concurrency = 5,
): Promise<BulkOutcome<T, R>> {
  if (items.length === 0) return { succeeded: [], failed: [] };

  const slots = Math.max(1, Math.min(concurrency, items.length));
  const results = new Array<{ item: T; value: R } | undefined>(items.length);
  const failures: { item: T; error: unknown }[] = [];
  let cursor = 0;

  async function worker() {
    while (true) {
      const i = cursor++;
      if (i >= items.length) return;
      const item = items[i];
      try {
        const value = await fn(item);
        results[i] = { item, value };
      } catch (error) {
        failures.push({ item, error });
      }
    }
  }

  const workers = Array.from({ length: slots }, worker);
  await Promise.all(workers);

  const succeeded = results.filter((r): r is { item: T; value: R } => r !== undefined);
  return { succeeded, failed: failures };
}
