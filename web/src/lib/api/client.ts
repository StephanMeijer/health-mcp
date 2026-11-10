import { loginUrl } from '#lib/server-routes.js';
import { ownerTools, type ToolArgs, type ToolName } from './tools.gen.js';

/** The caller's mistake (bad arguments, not allowed, not found): `message` is meant for the person. */
export class ToolUserError extends Error {
	override name = 'ToolUserError';
}

/** Nothing ran on the server (overloaded, restarting): trying again is safe. */
export class ToolRetryError extends Error {
	override name = 'ToolRetryError';
}

/** A server-side failure; details are in the server log. */
export class ToolServerError extends Error {
	override name = 'ToolServerError';
}

export interface ToolOptions {
	/** Run on this logbook (the owner's email) instead of your own; only for tools that take `owner`. */
	owner?: string | null;
	fetch?: typeof fetch;
	/** Where a 401 sends the browser; tests replace it. */
	onLoggedOut?: () => void;
}

function toLogin() {
	location.assign(loginUrl(location.pathname + location.search));
}

/**
 * Calls a tool over `/api/v1/tools/{name}` with the session cookie, as the web app. Same tools, arguments and
 * results as Claude gets. Resolves with the tool's JSON result. On a 401 the browser is sent to the login and this never settles, so
 * screens do not flash an error before the page unloads.
 */
export async function tool<N extends ToolName>(
	name: N,
	args: Omit<ToolArgs[N], 'owner'>,
	opts: ToolOptions = {}
): Promise<unknown> {
	const body: Record<string, unknown> = { ...args };
	if (opts.owner && ownerTools.has(name)) body.owner = opts.owner;
	const res = await (opts.fetch ?? fetch)(`/api/v1/tools/${name}`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { 'content-type': 'application/json', 'x-requested-with': 'health-web' },
		body: JSON.stringify(body)
	});
	if (res.ok) return res.json();
	const error = await res
		.json()
		.then((b: { error?: string }) => b.error)
		.catch(() => undefined);
	switch (res.status) {
		case 400:
			throw new ToolUserError(error ?? 'invalid request');
		case 401:
			(opts.onLoggedOut ?? toLogin)();
			return new Promise<never>(() => {});
		case 503:
			throw new ToolRetryError(error ?? 'temporarily unavailable');
		default:
			throw new ToolServerError(error ?? `the server failed (${res.status})`);
	}
}

/** For TanStack Query's `retry`: only transient failures, at most 3 times. */
export function retryable(failures: number, error: unknown): boolean {
	return error instanceof ToolRetryError && failures < 3;
}
