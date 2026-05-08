import { runWithConcurrency } from './concurrency';

describe('runWithConcurrency', () => {
  it('returns empty result for empty input without invoking the worker', async () => {
    const fn = vi.fn();
    const out = await runWithConcurrency([], fn);
    expect(out).toEqual({ succeeded: [], failed: [] });
    expect(fn).not.toHaveBeenCalled();
  });

  it('processes every item with the given fn', async () => {
    const fn = vi.fn(async (n: number) => n * 2);
    const out = await runWithConcurrency([1, 2, 3, 4], fn);
    expect(out.failed).toEqual([]);
    expect(out.succeeded.map((s) => s.value).sort()).toEqual([2, 4, 6, 8]);
  });

  it('reports partial failure without throwing', async () => {
    const fn = async (n: number) => {
      if (n % 2 === 0) throw new Error(`even ${n}`);
      return n * 10;
    };
    const out = await runWithConcurrency([1, 2, 3, 4], fn);
    expect(out.succeeded).toHaveLength(2);
    expect(out.failed).toHaveLength(2);
    expect(out.succeeded.map((s) => s.value).sort((a, b) => a - b)).toEqual([10, 30]);
    expect(out.failed.map((f) => f.item).sort((a, b) => a - b)).toEqual([2, 4]);
  });

  it('respects the concurrency cap (never more than N in flight)', async () => {
    let inFlight = 0;
    let peak = 0;
    const release: Array<() => void> = [];

    const fn = (n: number) =>
      new Promise<number>((resolve) => {
        inFlight++;
        peak = Math.max(peak, inFlight);
        release.push(() => {
          inFlight--;
          resolve(n);
        });
      });

    const items = Array.from({ length: 10 }, (_, i) => i);
    const promise = runWithConcurrency(items, fn, 3);

    // Allow the workers to start and saturate the slot count.
    await new Promise((r) => setTimeout(r, 0));
    expect(inFlight).toBeLessThanOrEqual(3);

    // Release everything in order; the workers should drain serially per slot.
    while (release.length > 0) {
      release.shift()?.();
      await new Promise((r) => setTimeout(r, 0));
    }
    await promise;
    expect(peak).toBeLessThanOrEqual(3);
  });

  it('preserves input order in the succeeded array', async () => {
    // Even if later items resolve faster, the result keeps input order so
    // callers can correlate results with their UI rows.
    const fn = (n: number) =>
      new Promise<number>((resolve) => {
        setTimeout(() => resolve(n), n % 2 === 0 ? 0 : 5);
      });
    const out = await runWithConcurrency([3, 1, 2, 0], fn, 2);
    expect(out.succeeded.map((s) => s.value)).toEqual([3, 1, 2, 0]);
  });
});
