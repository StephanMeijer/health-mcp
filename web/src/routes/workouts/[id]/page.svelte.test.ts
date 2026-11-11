import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tool, ToolUserError } from '#lib/api/client.js';
import { set, workout } from '#lib/test/workouts.js';
import WithQuery from '#lib/test/with-query.svelte';
import Page from './+page.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));
vi.mock('$app/state', () => ({ page: { params: { id: '4' } } }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => `/app${p}` }));

const mocked = vi.mocked(tool);
const withSet = {
	...workout,
	set_count: 1,
	exercises: [{ exercise_id: 7, exercise: 'Leg press', sets: [set] }]
};

beforeEach(() => mocked.mockReset());

describe('workout page', () => {
	it('loads the workout and its body notes by id', async () => {
		mocked.mockImplementation(async (name) =>
			name === 'workout_get' ? withSet : name === 'body_note_list' ? [] : []
		);
		render(WithQuery, { props: { component: Page } });
		expect(await screen.findByText('Leg press')).toBeInTheDocument();
		expect(mocked).toHaveBeenCalledWith('workout_get', { workout_id: 4 }, expect.anything());
		expect(mocked).toHaveBeenCalledWith('body_note_list', { workout_id: 4 }, expect.anything());
	});

	it('says so when the workout does not exist', async () => {
		mocked.mockImplementation(async (name) => {
			if (name === 'workout_get') throw new ToolUserError('no workout with id 4');
			return [];
		});
		render(WithQuery, { props: { component: Page } });
		expect(await screen.findByText(/doesn’t exist \(any more\)/)).toBeInTheDocument();
	});
});
