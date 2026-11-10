import { describe, expect, it, vi } from 'vitest';
import { access, logbook, type Shared } from './logbook.svelte.js';

const shared: Shared[] = [
	{ owner: 'ann@example.com', gym: 'rw', vitals: 'none' },
	{ owner: 'bob@example.com', gym: 'ro', vitals: 'ro' }
];

describe('access', () => {
	it('is full on your own logbook', () => {
		expect(access(null, [], 'gym')).toBe('rw');
		expect(access(null, shared, 'vitals')).toBe('rw');
	});

	it('follows the share per domain', () => {
		expect(access('ann@example.com', shared, 'gym')).toBe('rw');
		expect(access('ann@example.com', shared, 'vitals')).toBe('none');
		expect(access('bob@example.com', shared, 'gym')).toBe('ro');
	});

	it('is none for a logbook not shared with you', () => {
		expect(access('eve@example.com', shared, 'gym')).toBe('none');
	});
});

describe('refreshMe', () => {
	it('updates me and drops a logbook no longer shared with you', async () => {
		const me = { sub: 'u1', email: 'me@example.com', shared_with_you: [shared[1]] };
		const fetch = vi.fn(async () => new Response(JSON.stringify(me)));
		vi.stubGlobal('fetch', fetch);
		logbook.owner = 'ann@example.com';
		await logbook.refreshMe();
		expect(fetch).toHaveBeenCalledWith('/app/me', { credentials: 'same-origin' });
		expect(logbook.me).toEqual(me);
		expect(logbook.owner).toBeNull();

		logbook.owner = 'bob@example.com';
		await logbook.refreshMe();
		expect(logbook.owner).toBe('bob@example.com');
		vi.unstubAllGlobals();
	});
});
