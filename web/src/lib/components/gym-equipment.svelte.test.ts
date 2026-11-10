import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Gym } from '#lib/api/results.js';
import WithQuery from '#lib/test/with-query.svelte';
import GymEquipment from './gym-equipment.svelte';

const gym: Gym = {
	id: 3,
	name: 'Trainmore NDSM',
	aliases: [],
	equipment: 'Dumbbells 2-40 kg',
	notes: null,
	last_workout_at: null
};

afterEach(() => vi.unstubAllGlobals());

function setup() {
	const fetch = vi.fn(async () => Response.json(gym));
	vi.stubGlobal('fetch', fetch);
	const body = () =>
		JSON.parse((fetch.mock.calls.at(-1) as unknown as [string, RequestInit])[1].body as string);
	return { fetch, body, user: userEvent.setup({ pointerEventsCheck: 0 }) };
}

describe('GymEquipment', () => {
	it('"Add a line" appends to the equipment text', async () => {
		const { body, user } = setup();
		render(WithQuery, { props: { component: GymEquipment, props: { gym, editable: true } } });
		await user.type(screen.getByLabelText('Add a line'), '  Cable tower  ');
		await user.click(screen.getByRole('button', { name: 'Add line' }));
		expect(body()).toEqual({ gym: '3', equipment_append: 'Cable tower' });
	});

	it('"Edit all" replaces the whole text', async () => {
		const { body, user } = setup();
		render(WithQuery, { props: { component: GymEquipment, props: { gym, editable: true } } });
		await user.click(screen.getByRole('button', { name: 'Edit all' }));
		const all = screen.getByLabelText('All equipment, one thing per line');
		expect(all).toHaveValue('Dumbbells 2-40 kg');
		await user.clear(all);
		await user.type(all, 'Dumbbells 2-50 kg');
		await user.click(screen.getByRole('button', { name: 'Save equipment' }));
		expect(body()).toEqual({ gym: '3', equipment: 'Dumbbells 2-50 kg' });
	});

	it('shows the text without edit controls when read-only', () => {
		setup();
		render(WithQuery, { props: { component: GymEquipment, props: { gym, editable: false } } });
		expect(screen.getByText('Dumbbells 2-40 kg')).toBeInTheDocument();
		expect(screen.queryByRole('button')).toBeNull();
	});
});
