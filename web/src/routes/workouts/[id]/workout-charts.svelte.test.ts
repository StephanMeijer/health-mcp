import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tool } from '#lib/api/client.js';
import { logbook } from '#lib/logbook.svelte.js';
import { set, workout } from '#lib/test/workouts.js';
import WithQuery from '#lib/test/with-query.svelte';
import Charts from './workout-charts.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));
vi.mock('#lib/charts/echart.svelte', async () => ({
	default: (await import('#lib/test/echart-stub.svelte')).default
}));

const mocked = vi.mocked(tool);
const sqlOf = (args: object) => ('sql' in args ? String(args.sql) : '');
const w = {
	...workout,
	set_count: 2,
	exercises: [
		{
			exercise_id: 7,
			exercise: 'Leg press',
			sets: [set, { ...set, set_id: 12, set_number: 2, performed_at: '2026-11-10T18:15:00Z' }]
		}
	]
};
const start = Date.parse(workout.started_at) / 1000;

beforeEach(() => {
	mocked.mockReset();
	mocked.mockImplementation(async (_name, args) => {
		const sql = sqlOf(args);
		if (sql.includes('FROM exercise e'))
			return {
				truncated: false,
				rows: [
					{
						id: 7,
						kind: 'strength',
						muscle_groups: ['quads', 'glutes'],
						secondary_muscle_groups: [],
						first_at: start + 600
					}
				]
			};
		if (sql.includes('heart_rate'))
			return {
				truncated: false,
				rows: [
					{ t: start, bpm: 120 },
					{ t: start + 60, bpm: 160 }
				]
			};
		return { truncated: false, rows: [] };
	});
});
afterEach(() => {
	logbook.me = null;
	logbook.owner = null;
});

describe('workout overview', () => {
	it('shows the numbers, the heart rate summary and first-time exercises', async () => {
		render(WithQuery, { props: { component: Charts, props: { workout: w } } });
		expect(
			await screen.findByRole('img', {
				name: /60-minute workout: average 140 bpm, peak 160 bpm; 2 sets in 1 exercise blocks/
			})
		).toBeInTheDocument();
		expect(screen.getByText('max 160 bpm')).toBeInTheDocument();
		expect(screen.getByText('400')).toBeInTheDocument(); // 2 × 10 × 20 kg
		expect(screen.getByText(/120–139 bpm · 1 min/)).toBeInTheDocument();
		expect(screen.getByText('Legs 100% · 4 sets')).toBeInTheDocument();
		expect(screen.getAllByText('Leg press').length).toBeGreaterThan(1); // set grid and first-time badge
	});

	it('keeps the blocks but asks nothing of vitals that are not shared', async () => {
		logbook.me = {
			sub: 'u1',
			email: 'me@example.com',
			shared_with_you: [{ owner: 'ann@example.com', gym: 'ro', vitals: 'none' }]
		};
		logbook.owner = 'ann@example.com';
		render(WithQuery, { props: { component: Charts, props: { workout: w } } });
		expect(await screen.findByText(/doesn't share with you/)).toBeInTheDocument();
		expect(screen.getByRole('img', { name: /no heart rate recorded/ })).toBeInTheDocument();
		expect(screen.queryByText('Heart rate')).toBeNull();
		expect(mocked.mock.calls.some(([, a]) => sqlOf(a).includes('heart_rate'))).toBe(false);
	});
});
