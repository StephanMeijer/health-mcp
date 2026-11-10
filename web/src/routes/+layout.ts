import { loginUrl, meUrl } from '#lib/server-routes.js';
import { error } from '@sveltejs/kit';
import type { Me } from '#lib/logbook.svelte.js';
import type { LayoutLoad } from './$types';

// A single-page app behind a login: nothing is rendered or prerendered on the server.
export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ fetch, url }) => {
	const res = await fetch(meUrl);
	if (res.status === 401) {
		location.assign(loginUrl(url.pathname + url.search));
		return new Promise<never>(() => {});
	}
	if (!res.ok) error(res.status, 'Your account could not be loaded. Try again in a moment.');
	return { me: (await res.json()) as Me };
};
