/**
 * Phase 7 — a mocked backend for the autopilot tests: `fetch` answers by the route asked for, and every request is kept so a test
 * can say exactly what was sent. Test-only.
 */
import { vi } from 'vitest';

export function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

export function textResponse(status: number, body: string): Response {
  return new Response(body, { status, headers: { 'Content-Type': 'text/markdown; charset=utf-8' } });
}

/** The backend's own error shape (`errors.rs`). */
export function failure(status: number, code: string, message: string): Response {
  return jsonResponse(status, { error: { code, message, details: {}, request_id: 'req_autopilot_test' } });
}

export interface Req {
  path: string;
  method: string;
  query: Record<string, string>;
  body: any;
}

/** What a handler may return: a `Response`, or a value that is sent as a 200 JSON body. */
type Reply = Response | unknown;
export type Handler = (req: Req) => Reply | Promise<Reply>;

/**
 * `fetch` that answers by `"METHOD /route-tail"` (the request path only has to end with the tail). A route with no handler
 * answers an empty list, which is what every page's incidental reads (skills, providers) get.
 */
export function backend(routes: Record<string, Handler>) {
  const entries = Object.entries(routes).map(([key, handler]) => {
    const space = key.indexOf(' ');
    return { method: key.slice(0, space), tail: key.slice(space + 1), handler };
  });
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    const parsed = new URL(url, 'http://localhost');
    const req: Req = {
      path: parsed.pathname,
      method: (init?.method ?? 'GET').toUpperCase(),
      query: Object.fromEntries(parsed.searchParams.entries()),
      body: init?.body ? JSON.parse(String(init.body)) : undefined,
    };
    // The most specific (longest) tail wins, so `POST /runs/x/start` is not taken for `POST /runs`.
    const found = entries
      .filter((e) => e.method === req.method && (e.tail.endsWith('$') ? req.path.endsWith(e.tail.slice(0, -1)) : req.path.includes(e.tail)))
      .sort((a, b) => b.tail.length - a.tail.length)[0];
    if (!found) return jsonResponse(200, []);
    const reply = await found.handler(req);
    return reply instanceof Response ? reply : jsonResponse(200, reply);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

/** The requests a mock saw whose method and path match `"METHOD /tail"` (a tail ending `$` must be the end of the path). */
export function requests(fetchMock: ReturnType<typeof vi.fn>, route: string): Req[] {
  const space = route.indexOf(' ');
  const method = route.slice(0, space);
  const tail = route.slice(space + 1);
  return fetchMock.mock.calls
    .map(([url, init]: [string, RequestInit | undefined]) => {
      const parsed = new URL(url, 'http://localhost');
      return {
        path: parsed.pathname,
        method: (init?.method ?? 'GET').toUpperCase(),
        query: Object.fromEntries(parsed.searchParams.entries()),
        body: init?.body ? JSON.parse(String(init.body)) : undefined,
      } as Req;
    })
    .filter((r) => r.method === method && (tail.endsWith('$') ? r.path.endsWith(tail.slice(0, -1)) : r.path.includes(tail)));
}
