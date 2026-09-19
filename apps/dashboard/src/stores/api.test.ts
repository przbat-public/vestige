import { api } from './api';

/**
 * `/api/health` reports its own severity through the HTTP status: 200 for
 * healthy and degraded, 503 for critical (see
 * `crates/vestige-mcp/src/dashboard/handlers/observability.rs`). The generic
 * fetcher throws on any non-2xx, so before this test the critical case rejected
 * and `StatsPage`/`SettingsPage` lost the DTO body — the red pill and the
 * version — in exactly the state an operator opens those pages to diagnose.
 */
describe('api.health', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const healthBody = {
    status: 'critical',
    version: '3.4.0',
    database: { status: 'ok' },
  };

  function stubFetch(status: number, body: unknown) {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({
        ok: status >= 200 && status < 300,
        status,
        headers: new Headers({ 'content-type': 'application/json', 'content-length': '100' }),
        json: async () => body,
      })),
    );
  }

  it('resolves with the DTO when the store is critical (HTTP 503)', async () => {
    stubFetch(503, healthBody);

    await expect(api.health()).resolves.toMatchObject({ status: 'critical', version: '3.4.0' });
  });

  it('still resolves for a healthy store (HTTP 200)', async () => {
    stubFetch(200, { ...healthBody, status: 'healthy' });

    await expect(api.health()).resolves.toMatchObject({ status: 'healthy' });
  });

  it('still rejects a genuine failure, so tolerating 503 does not swallow errors', async () => {
    stubFetch(500, { error: { code: 'internal', message: 'boom' } });

    // ApiError carries the server's message when the body has one.
    await expect(api.health()).rejects.toThrow('boom');
  });
});
