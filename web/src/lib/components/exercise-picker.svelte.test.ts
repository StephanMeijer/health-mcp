import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Exercise } from '#lib/api/results.js';
import WithQuery from '#lib/test/with-query.svelte';
import ExercisePicker from './exercise-picker.svelte';

const bench: Exercise = {
	id: 7,
	name: 'Dumbbell bench press',
	aliases: [],
	kind: 'strength',
	description: null,
	muscle_groups: ['chest'],
	secondary_muscle_groups: ['triceps'],
	equipment: 'Dumbbells + flat bench'
};
const row = { ...bench, id: 8, name: 'Barbell row', muscle_groups: ['lats'] };

afterEach(() => vi.unstubAllGlobals());

describe('ExercisePicker', () => {
	it('searches as you type and hands back the exercise you pick', async () => {
		const fetch = vi.fn(async () => Response.json([bench, row]));
		vi.stubGlobal('fetch', fetch);
		const onpick = vi.fn();
		const user = userEvent.setup();
		render(WithQuery, { props: { component: ExercisePicker, props: { onpick } } });

		await user.type(screen.getByRole('combobox', { name: 'Exercise' }), 'benchpres');
		await user.click(await screen.findByRole('option', { name: /Dumbbell bench press/ }));

		expect(fetch).toHaveBeenCalledWith(
			'/api/v1/tools/exercise_search',
			expect.objectContaining({ body: JSON.stringify({ query: 'benchpres' }) })
		);
		expect(onpick).toHaveBeenCalledWith(bench);
	});

	it('picks with the arrow keys and Enter', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => Response.json([bench, row]))
		);
		const onpick = vi.fn();
		const user = userEvent.setup();
		render(WithQuery, { props: { component: ExercisePicker, props: { onpick } } });

		await user.type(screen.getByRole('combobox'), 'b');
		await screen.findAllByRole('option');
		await user.keyboard('{ArrowDown}{Enter}');
		expect(onpick).toHaveBeenCalledWith(row);
	});
});
