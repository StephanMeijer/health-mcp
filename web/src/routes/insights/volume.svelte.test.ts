import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tool } from '#lib/api/client.js';
import { logbook, type Shared } from '#lib/logbook.svelte.js';
import WithQuery from '#lib/test/with-query.svelte';
import Volume from './volume.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));

const mocked = vi.mocked(tool);
const sqlOf = (args: object) => ('sql' in args ? String(args.sql) : '');
// Twelve weeks with neither sets nor HRV, as the generate_series query returns them.
const emptyWeeks = Array.from({ length: 12 }, (_, i) => ({
	week: `2026-08-${String(i + 1).padStart(2, '0')}T00:00:00+00:00`,
	sets: 0,
	avg_rmssd_ms: null
}));

function sharedBy(share: Shared | null) {
	logbook.me = { sub: 'u1', email: 'me@example.com', shared_with_you: share ? [share] : [] };
	logbook.owner = share?.owner ?? null;
}

beforeEach(() => {
	mocked.mockReset();
	mocked.mockImplementation(async (name, args) => {
		if (name === 'muscle_group_volume') return { muscle_groups: [] };
		if (name === 'sql_query')
			return { truncated: false, rows: sqlOf(args).includes('hrv') ? emptyWeeks : [] };
		return [];
	});
});
afterEach(() => sharedBy(null));

describe('Volume tab', () => {
	it('explains a vitals-only share instead of asking the server for gym data', async () => {
		sharedBy({ owner: 'ann@example.com', gym: 'none', vitals: 'ro' });
		render(WithQuery, { component: Volume });
		expect(screen.getByText("This logbook's gym data isn't shared with you.")).toBeTruthy();
		expect(mocked).not.toHaveBeenCalled();
	});

	it('leaves out HRV on a gym-only share', async () => {
		sharedBy({ owner: 'ann@example.com', gym: 'ro', vitals: 'none' });
		render(WithQuery, { component: Volume });
		expect(await screen.findByText(/HRV is part of the vitals/)).toBeTruthy();
		await screen.findByText('No training days in the last year');
		expect(mocked.mock.calls.some(([, a]) => sqlOf(a).includes('hrv'))).toBe(false);
	});

	it('shows the empty state when all twelve weeks have no sets and no HRV', async () => {
		render(WithQuery, { component: Volume });
		expect(await screen.findByText('No sets or HRV in the last 12 weeks')).toBeTruthy();
	});
});
