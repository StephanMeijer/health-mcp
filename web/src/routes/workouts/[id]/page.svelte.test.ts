import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
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

	it('shows any other refusal as it is', async () => {
		mocked.mockImplementation(async (name) => {
			if (name === 'workout_get')
				throw new ToolUserError('jamie@trainer.nl has not shared their gym data with you');
			return [];
		});
		render(WithQuery, { props: { component: Page } });
		expect(await screen.findByText(/has not shared their gym data with you/)).toBeInTheDocument();
		expect(screen.queryByText(/doesn’t exist/)).toBeNull();
	});

	it('counts each set’s comments from one call and opens them in a side panel', async () => {
		const setThread = {
			thread_id: 5,
			target: 'set',
			workout_id: null,
			exercise_id: null,
			set_id: 11,
			exercise: 'Leg press',
			resolved: false,
			resolved_at: null,
			created_at: '2026-11-10T19:00:00Z',
			can_resolve: true,
			comments: [
				{
					id: 1,
					author: 'jamie@trainer.nl',
					by_owner: false,
					yours: false,
					body: 'Go deeper',
					deleted: false,
					created_at: '2026-11-10T19:00:00Z',
					edited_at: null
				},
				{
					id: 2,
					author: 'steve@example.com',
					by_owner: true,
					yours: true,
					body: null,
					deleted: true,
					created_at: '2026-11-10T19:05:00Z',
					edited_at: null
				}
			]
		};
		mocked.mockImplementation(async (name) =>
			name === 'workout_get' ? withSet : name === 'comment_list' ? [setThread] : []
		);
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, { props: { component: Page } });
		await user.click(await screen.findByRole('button', { name: '1 comment on set 1' }));
		expect(await screen.findByText('Go deeper')).toBeInTheDocument();
		expect(mocked.mock.calls.filter(([name]) => name === 'comment_list')).toEqual([
			['comment_list', { workout_id: 4, include_resolved: true, limit: 100 }, expect.anything()]
		]);
	});
});
