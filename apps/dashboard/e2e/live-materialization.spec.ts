/**
 * Dashboard end-to-end suite.
 *
 * Rewritten from the 2026-09 review finding that the previous eleven tests
 * were unrunnable outside one developer's Mac and that seven of them asserted
 * nothing (their whole body was `page.screenshot()`). The rules this file
 * follows now:
 *
 * 1. **Self-contained.** `playwright.config.ts` starts the `vestige-mcp`
 *    binary built from this repository on throwaway ports with a temporary
 *    data directory and a token supplied through `VESTIGE_AUTH_TOKEN`. No
 *    hardcoded absolute paths, no macOS-only token file, no assumption that a
 *    server is already running, and nothing that can reach the developer's own
 *    database.
 * 2. **Every test asserts an observable outcome** — rendered text, a network
 *    effect, or persisted server state. Screenshots are produced by Playwright
 *    on failure, not by the test body.
 * 3. **Cleanup deletes only ids this run created** (tracked in `createdIds`).
 *    The old suite deleted every memory whose content contained "E2E TEST",
 *    which is an irreversible operation on real data.
 * 4. **Playwright auto-waits.** No `waitForTimeout`: assertions poll for the
 *    WebSocket-driven refetch instead of sleeping a fixed number of
 *    milliseconds and hoping.
 */
import { expect, test } from '@playwright/test';
import {
  deleteMemory,
  fetchGraph,
  fetchMemories,
  fetchMemory,
  ingestViaMcp,
  newMarker,
} from './api';
import { E2E } from './env';

/** Ids created by the running test; removed in `afterEach`. */
const createdIds: string[] = [];

async function ingest(content: string, tags: string[] = ['e2e']): Promise<string> {
  const id = await ingestViaMcp(content, { tags });
  createdIds.push(id);
  return id;
}

test.afterEach(async () => {
  while (createdIds.length > 0) {
    const id = createdIds.pop();
    if (id) await deleteMemory(id);
  }
});

test.describe('Live materialization — an external MCP client drives the UI', () => {
  test('a memory ingested over MCP appears in the memories list without a reload', async ({ page }) => {
    const marker = newMarker('live list');

    await page.goto(E2E.memoriesPath);
    await expect(page.getByRole('textbox', { name: 'Search memories...' })).toBeVisible();
    await expect(page.getByText(marker, { exact: false })).toHaveCount(0);

    const id = await ingest(marker, ['e2e-live']);

    // The server broadcasts MemoryCreated on /ws, the store invalidates the
    // `['memories']` query, and the row renders in the page that was already
    // open — that round trip is the behaviour under test.
    await expect(page.getByText(marker, { exact: false }).first()).toBeVisible({ timeout: 15_000 });

    // ...and the row came from the server, not from optimistic client state.
    expect((await fetchMemory(id)).content).toBe(marker);
  });

  test('the activity feed shows the MemoryCreated event raised by an MCP ingest', async ({ page }) => {
    const marker = newMarker('feed event');

    await page.goto(E2E.feedPath);
    await expect(page.getByTestId('feed-event')).toHaveCount(0);

    const id = await ingest(marker, ['e2e-feed']);

    const row = page.getByTestId('feed-event').filter({ hasText: 'MemoryCreated' }).first();
    await expect(row).toBeVisible({ timeout: 15_000 });
    // The payload carries the id of the memory that was just written, which
    // ties the broadcast to this exact ingest. (The `contentPreview` field is
    // empty for tool-driven writes, so the id is the assertion that actually
    // distinguishes one event from another.)
    await expect(row).toContainText(id);
  });

  test('a memory deleted by another client leaves the open list', async ({ page }) => {
    const marker = newMarker('remote removal');
    const id = await ingest(marker, ['e2e-live']);

    await page.goto(E2E.memoriesPath);
    await expect(page.getByText(marker, { exact: false }).first()).toBeVisible();

    expect(await deleteMemory(id), 'DELETE /api/memories/{id} must report success').toBe(true);

    await expect(page.getByText(marker, { exact: false })).toHaveCount(0, { timeout: 15_000 });
  });
});

test.describe('Memory CRUD through the dashboard', () => {
  test('the add-memory dialog persists a memory and lists it', async ({ page }) => {
    const marker = newMarker('ui create');

    await page.goto(E2E.memoriesPath);
    await expect(page.getByRole('textbox', { name: 'Search memories...' })).toBeVisible();

    // Ctrl/Cmd+N is the documented shortcut for the primary write action.
    await page.keyboard.press('ControlOrMeta+n');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    await dialog.getByLabel('Content', { exact: true }).fill(marker);
    await dialog.getByLabel('Tags', { exact: true }).fill('e2e-ui');
    await dialog.getByRole('button', { name: 'Save memory' }).click();

    await expect(dialog).toBeHidden({ timeout: 15_000 });
    await expect(page.getByText(marker, { exact: false }).first()).toBeVisible({ timeout: 15_000 });

    const created = (await fetchMemories()).memories.find((memory) => memory.content === marker);
    expect(created, 'the UI-created memory must be readable through the REST API').toBeTruthy();
    if (created) createdIds.push(created.id);
  });

  test('deleting from the detail panel removes the memory from the list and the API', async ({ page }) => {
    // Marker wording matters: accessible-name matching is substring-based, so a
    // content containing the word "delete" makes the row button itself match
    // `getByRole('button', { name: 'Delete' })` and the click below would hit
    // the wrong control.
    const marker = newMarker('ui removal');
    const id = await ingest(marker, ['e2e-ui']);

    await page.goto(E2E.memoriesPath);
    const row = page.locator(`[data-memory-row="${id}"]`);
    await expect(row).toContainText(marker);
    // The row body is a button; the label that wraps the bulk-select checkbox
    // sits on top of the leading edge, so clicking the raw text would toggle
    // selection instead of opening the detail panel.
    await row.getByRole('button').first().click();

    // `exact` keeps the panel's Delete button from competing with any other
    // button whose name merely contains the word.
    const panelDelete = page.getByRole('button', { name: 'Delete', exact: true });
    await panelDelete.click();

    // Delete is deferred: the row leaves the list immediately, but the actual
    // DELETE fires only after the five-second undo window, so a row that
    // merely vanished from the cache has deleted nothing. Assert both halves.
    await expect(row).toHaveCount(0);
    await expect
      .poll(
        async () => {
          try {
            await fetchMemory(id);
            return 'present';
          } catch {
            return 'gone';
          }
        },
        { timeout: 20_000, message: 'the deferred delete was never committed to the server' },
      )
      .toBe('gone');
    // Once the server confirms, `onDelete` closes the detail panel, which
    // falls back to its "Select a memory" placeholder.
    await expect(panelDelete).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'Select a memory to view details' })).toBeVisible();
  });
});

test.describe('Graph view', () => {
  test('the graph renders exactly the node set the API returns', async ({ page }) => {
    await ingest(newMarker('graph parity a'), ['e2e-graph']);
    await ingest(newMarker('graph parity b'), ['e2e-graph']);

    const expected = await fetchGraph();
    await page.goto(E2E.graphPath);

    const graph = page.getByRole('application', { name: /memory graph canvas/i });
    await expect(graph).toBeVisible();
    await expect(graph.locator('canvas')).toBeVisible();

    // The stats bar is derived from the same payload the test just fetched:
    // exact numbers, not "at least as many as before".
    await expect(page.getByText(`${expected.nodeCount} nodes`, { exact: true })).toBeVisible();
    await expect(page.getByText(`${expected.edgeCount} connections`, { exact: true })).toBeVisible();

    // Every node is mirrored into an sr-only list (aria-activedescendant
    // targets), which is the only DOM representation of the 3D scene.
    const renderedLabels = graph.getByRole('listitem');
    await expect(renderedLabels).toHaveCount(expected.nodeCount);
    expect(new Set(await renderedLabels.allTextContents())).toEqual(
      new Set(expected.nodes.map((node) => node.label)),
    );
  });

  test('searching the graph materializes an MCP-created memory and keyboard Enter opens it', async ({ page }) => {
    const marker = newMarker('graph search');
    await ingest(marker, ['e2e-graph']);

    await page.goto(E2E.graphPath);
    const graph = page.getByRole('application', { name: /memory graph canvas/i });
    await expect(graph).toBeVisible();

    await page.getByRole('textbox', { name: 'Search your memories...' }).fill(marker);
    await page.getByRole('button', { name: 'Search', exact: true }).click();

    // The backend centres the subgraph on the search hit, so exactly one node
    // is on screen and it is the memory the external client just wrote.
    const renderedLabels = graph.getByRole('listitem');
    await expect(renderedLabels).toHaveCount(1);
    await expect(renderedLabels.first()).toHaveText(marker);

    // Keyboard contract: arrows/Home move a cursor, the aria-live region
    // announces it, Enter opens the detail sheet.
    await graph.focus();
    await page.keyboard.press('Home');
    await expect(page.getByRole('status').filter({ hasText: 'Focused node:' })).toHaveText(
      `Focused node: ${marker}`,
    );

    await page.keyboard.press('Enter');
    const detailHeading = page.getByRole('heading', { name: 'Selected' });
    await expect(detailHeading).toBeVisible();
    // The sheet renders the header row and the memory body as siblings, so
    // asserting on the header's grandparent covers the whole panel.
    await expect(detailHeading.locator('xpath=../..')).toContainText(marker);

    await page.keyboard.press('Escape');
    await expect(detailHeading).toBeHidden();
  });

  test('the tag filter can empty the graph and the reset control restores it', async ({ page }) => {
    await ingest(newMarker('tag filter'), ['e2e-graph']);
    const expected = await fetchGraph();

    await page.goto(E2E.graphPath);
    const graph = page.getByRole('application', { name: /memory graph canvas/i });
    const renderedLabels = graph.getByRole('listitem');
    await expect(renderedLabels).toHaveCount(expected.nodeCount);

    const tagFilter = page.getByRole('textbox', { name: 'Filter by tag' });
    await tagFilter.fill('e2e-tag-that-matches-nothing');

    await expect(renderedLabels).toHaveCount(0);
    await expect(page.getByText('0 nodes', { exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'No nodes match the current filter' })).toBeVisible();

    await page.getByRole('button', { name: 'Clear filter' }).click();
    await expect(renderedLabels).toHaveCount(expected.nodeCount);
  });

  test('sidebar navigation switches between the memories page and the graph', async ({ page }) => {
    await ingest(newMarker('navigation'), ['e2e-graph']);

    await page.goto(E2E.memoriesPath);
    await expect(page.getByRole('textbox', { name: 'Search memories...' })).toBeVisible();

    await page.getByRole('link', { name: 'Graph', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard\/graph$/);
    await expect(page.getByRole('application', { name: /memory graph canvas/i })).toBeVisible();

    await page.getByRole('link', { name: 'Memories', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard\/memories$/);
    await expect(page.getByRole('textbox', { name: 'Search memories...' })).toBeVisible();
  });
});
