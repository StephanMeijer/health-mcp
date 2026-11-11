import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tool, ToolUserError } from '#lib/api/client.js';
import type { GymListItem } from '#lib/api/results.js';
import { workout } from '#lib/test/workouts.js';
import WithQuery from '#lib/test/with-query.svelte';
import CurrentWorkout from './current-workout.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => `/app${p}` }));

const gyms: GymListItem[] = [
	{ id: 1, name: 'Gym A', aliases: [], last_workout_at: '2026-11-01T18:00:00Z' },
	{ id: 2, name: 'Gym B', aliases: [], last_workout_at: '2026-11-09T18:00:00Z' }
];
const open = { ...workout, finished_at: null, set_count: 3 };
const mocked = vi.mocked(tool);

beforeEach(() => {
	mocked.mockReset();
	mocked.mockImplementation(async (name, args) => {
		if (name === 'workout_current') return { open_workout: null, closed_stale: null };
		if (name === 'gym_list') return gyms;
		if (name === 'workout_start' && !(args as { finish_open?: boolean }).finish_open)
			throw new ToolUserError(
				JSON.stringify({ error: 'a workout is still open at Gym A (3 sets)', open_workout: open })
			);
		return { status: 'started', workout: { ...workout, workout_id: 9 }, closed_stale: null };
	});
});

describe('CurrentWorkout', () => {
	it('offers to finish a workout open at another gym and start here', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, { props: { component: CurrentWorkout } });
		await user.click(await screen.findByRole('button', { name: 'Start workout' }));
		expect(await screen.findByText(/Your workout at Gym A \(3 sets\) is still open/)).toBeVisible();
		await user.click(screen.getByRole('button', { name: 'Finish it and start here' }));
		const starts = mocked.mock.calls.filter(([n]) => n === 'workout_start').map(([, a]) => a);
		// The most recently trained gym is the default.
		expect(starts).toEqual([{ gym: '2' }, { gym: '2', finish_open: true }]);
		expect(screen.queryByText(/is still open/)).toBeNull();
	});
});
