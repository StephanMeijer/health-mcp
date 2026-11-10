import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { COARSE_GROUPS, MUSCLE_GROUPS, muscleLabel } from '#lib/muscles.js';
import MuscleGroupsInput from './muscle-groups-input.svelte';

describe('MuscleGroupsInput', () => {
	it('offers only the 18 fine muscle groups, never the coarse filter names', () => {
		render(MuscleGroupsInput, { id: 'p', label: 'Target muscles' });
		const boxes = screen.getAllByRole('checkbox');
		expect(boxes).toHaveLength(MUSCLE_GROUPS.length);
		for (const coarse of Object.keys(COARSE_GROUPS))
			expect(screen.queryByLabelText(muscleLabel(coarse))).toBeNull();
	});

	it('keeps primary and secondary lists apart', async () => {
		const user = userEvent.setup();
		render(MuscleGroupsInput, {
			id: 'p',
			label: 'Target muscles',
			value: ['chest'],
			disabled: []
		});
		render(MuscleGroupsInput, {
			id: 's',
			label: 'Muscles that assist',
			value: [],
			disabled: ['chest']
		});
		const chest = (prefix: string) => document.getElementById(`${prefix}-chest`)!;
		expect(chest('p')).toHaveAttribute('aria-checked', 'true');
		expect(chest('s')).toBeDisabled();

		await user.click(document.getElementById('p-triceps')!);
		expect(document.getElementById('p-triceps')).toHaveAttribute('aria-checked', 'true');
		await user.click(chest('p'));
		expect(chest('p')).toHaveAttribute('aria-checked', 'false');
	});
});
