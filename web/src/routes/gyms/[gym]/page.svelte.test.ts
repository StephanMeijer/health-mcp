import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Gym } from '#lib/api/results.js';
import WithQuery from '#lib/test/with-query.svelte';
import GymPage from './+page.svelte';

vi.mock('$app/state', () => ({ page: { params: { gym: '3' } } }));

const gym: Gym = {
	id: 3,
	name: 'Trainmore NDSM',
	aliases: [],
	equipment: null,
	notes: 'Busy after 17:00',
	last_workout_at: null
};

afterEach(() => vi.unstubAllGlobals());

describe('gym page', () => {
	it('"Edit gym" sends whitespace-only notes as an empty string', async () => {
		const fetch = vi.fn(async () => Response.json(gym));
		vi.stubGlobal('fetch', fetch);
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, { props: { component: GymPage } });
		await user.click(await screen.findByRole('button', { name: 'Edit gym' }));
		const notes = screen.getByLabelText('Notes');
		await user.clear(notes);
		await user.type(notes, '   ');
		await user.click(screen.getByRole('button', { name: 'Save gym' }));
		const calls = fetch.mock.calls as unknown as [string, RequestInit][];
		const [, init] = calls.find(([url]) => url === '/api/v1/tools/gym_update')!;
		expect(JSON.parse(init.body as string)).toEqual({
			gym: '3',
			name: 'Trainmore NDSM',
			aliases: [],
			notes: ''
		});
	});
});
