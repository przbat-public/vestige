/**
 * HTTP clients used by the e2e specs.
 *
 * Two transports are in play because that is how the product is used:
 *
 * - **MCP over HTTP** (`E2E.mcpUrl`, bearer token) plays the role of the
 *   external agent that writes memories. Everything the dashboard receives
 *   over its WebSocket comes from this path, so the "live" assertions mean
 *   something.
 * - **Dashboard REST** (`E2E.dashboardUrl/api/...`) is the same API the SPA
 *   calls. Specs use it to pin expectations to server truth (node counts,
 *   persisted content) instead of to what the page already showed.
 *
 * Every helper throws with the failing request baked into the message: a
 * suite that reports "expected 2, received 0" without saying which call
 * returned what is a debugging tax.
 */
import { randomUUID } from 'node:crypto';
import { E2E } from './env';

export interface MemoryRecord {
  id: string;
  content: string;
  nodeType: string;
  tags: string[];
  retentionStrength: number;
}

export interface MemoryListResponse {
  total: number;
  memories: MemoryRecord[];
}

export interface GraphNodeRecord {
  id: string;
  label: string;
  type: string;
  tags: string[];
  isCenter: boolean;
}

export interface GraphResponse {
  nodes: GraphNodeRecord[];
  edges: unknown[];
  centerId: string;
  nodeCount: number;
  edgeCount: number;
}

/** Unique, greppable marker so cleanup can never touch someone else's row. */
export function newMarker(label: string): string {
  return `E2E ${label} ${randomUUID().slice(0, 8)}`;
}

async function request(url: string, init?: RequestInit): Promise<Response> {
  const response = await fetch(url, init);
  if (!response.ok) {
    const body = await response.text().catch(() => '<unreadable body>');
    throw new Error(`${init?.method ?? 'GET'} ${url} failed with ${response.status}: ${body.slice(0, 500)}`);
  }
  return response;
}

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await request(url, init);
  return (await response.json()) as T;
}

export function fetchMemories(limit = 100): Promise<MemoryListResponse> {
  return json<MemoryListResponse>(`${E2E.dashboardUrl}/api/memories?limit=${limit}&offset=0`);
}

export function fetchMemory(id: string): Promise<MemoryRecord> {
  return json<MemoryRecord>(`${E2E.dashboardUrl}/api/memories/${id}`);
}

/** Same request the graph page issues with its default controls. */
export function fetchGraph(): Promise<GraphResponse> {
  return json<GraphResponse>(`${E2E.dashboardUrl}/api/graph?max_nodes=150&depth=2`);
}

export async function fetchSearch(query: string, limit = 5): Promise<{ results: MemoryRecord[] }> {
  return json<{ results: MemoryRecord[] }>(
    `${E2E.dashboardUrl}/api/search?q=${encodeURIComponent(query)}&limit=${limit}`,
  );
}

/** Returns false when the memory is already gone (cleanup after a UI delete). */
export async function deleteMemory(id: string): Promise<boolean> {
  // `?confirmed=true` mirrors the MCP `memory(action="delete")` gate the route
  // now enforces. This helper is test cleanup, so the acknowledgement is
  // unconditional and stands in for the operator's.
  const response = await fetch(`${E2E.dashboardUrl}/api/memories/${id}?confirmed=true`, { method: 'DELETE' });
  if (response.ok || response.status === 404) return response.ok;
  const body = await response.text().catch(() => '<unreadable body>');
  throw new Error(`DELETE /api/memories/${id} failed with ${response.status}: ${body.slice(0, 500)}`);
}

interface McpToolResult {
  nodeId?: string;
  decision?: string;
  success?: boolean;
}

let mcpSessionId: string | null = null;
let mcpCallId = 0;

async function mcpPost(body: unknown, sessionId?: string): Promise<Response> {
  return request(E2E.mcpUrl, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${E2E.token}`,
      ...(sessionId ? { 'Mcp-Session-Id': sessionId } : {}),
    },
    body: JSON.stringify(body),
  });
}

async function mcpSession(): Promise<string> {
  if (mcpSessionId) return mcpSessionId;

  const response = await mcpPost({
    jsonrpc: '2.0',
    id: ++mcpCallId,
    method: 'initialize',
    params: {
      protocolVersion: '2025-03-26',
      capabilities: {},
      clientInfo: { name: 'vestige-dashboard-e2e', version: '1.0.0' },
    },
  });

  const session = response.headers.get('mcp-session-id');
  if (!session) {
    throw new Error(
      `MCP initialize returned no Mcp-Session-Id header (status ${response.status}); ` +
        'the HTTP transport contract changed — the e2e suite cannot create memories.',
    );
  }
  mcpSessionId = session;

  await mcpPost({ jsonrpc: '2.0', method: 'notifications/initialized' }, session);
  return session;
}

async function mcpCall(tool: string, args: Record<string, unknown>): Promise<McpToolResult> {
  const session = await mcpSession();
  const data = (await (
    await mcpPost(
      { jsonrpc: '2.0', id: ++mcpCallId, method: 'tools/call', params: { name: tool, arguments: args } },
      session,
    )
  ).json()) as {
    result?: { content?: Array<{ text?: string }>; structuredContent?: McpToolResult; isError?: boolean };
    error?: unknown;
  };

  if (data.error) throw new Error(`MCP ${tool} failed: ${JSON.stringify(data.error)}`);
  if (data.result?.isError) {
    throw new Error(`MCP ${tool} returned isError: ${JSON.stringify(data.result.content)}`);
  }

  const structured = data.result?.structuredContent;
  if (structured) return structured;

  const text = data.result?.content?.[0]?.text;
  if (!text) throw new Error(`MCP ${tool} returned no structuredContent and no text content`);
  return JSON.parse(text) as McpToolResult;
}

/**
 * Create a memory through the MCP HTTP transport — the same path an agent
 * uses. Returns the node id; throws when the server did not mint one, so a
 * broken write surfaces at the call site rather than as a confusing UI
 * assertion three steps later.
 */
export async function ingestViaMcp(
  content: string,
  options: { tags?: string[]; nodeType?: string } = {},
): Promise<string> {
  const result = await mcpCall('smart_ingest', {
    content,
    tags: options.tags ?? [],
    node_type: options.nodeType ?? 'fact',
  });
  if (!result.nodeId) {
    throw new Error(`smart_ingest returned no nodeId for "${content}": ${JSON.stringify(result)}`);
  }
  return result.nodeId;
}
