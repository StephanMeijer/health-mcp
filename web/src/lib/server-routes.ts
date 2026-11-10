import { toast } from 'svelte-sonner';

/**
 * Routes health-mcp serves itself under /app (src/session.rs), next to this app. Not app pages, so not `resolve()`d:
 * they share the `paths.base` set in vite.config.ts.
 */
const BASE = '/app';

export const meUrl = `${BASE}/me`;
export const logoutUrl = `${BASE}/logout`;

/** The login page, returning to `next` (a path inside the app) afterwards. */
export function loginUrl(next?: string): string {
	return next ? `${BASE}/login?next=${encodeURIComponent(next)}` : `${BASE}/login`;
}

/**
 * Ends the session, then shows the login page. A 401 means the session was already gone. On any other failure the
 * cookie may still be valid, so the person stays here and is told, instead of believing they are logged out.
 */
export async function logout() {
	const res = await fetch(logoutUrl, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { 'x-requested-with': 'health-web' }
	}).catch(() => undefined);
	if (res && (res.ok || res.status === 401)) location.assign(loginUrl());
	else toast.error('Logging out failed; try again');
}
