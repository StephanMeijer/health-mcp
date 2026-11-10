import { describe, expect, it } from 'vitest';
import { access, type Shared } from './logbook.svelte.js';

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
