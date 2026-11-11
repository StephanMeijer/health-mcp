import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { set } from '#lib/test/workouts.js';
import SetRow from './set-row.svelte';

function setup(s = set) {
	const onsave = vi.fn(async () => {});
	const user = userEvent.setup({ pointerEventsCheck: 0 });
	render(SetRow, { set: s, editable: true, onsave });
	return { onsave, user };
}

describe('SetRow', () => {
	it('saves only the changed fields', async () => {
		const { onsave, user } = setup();
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		const reps = screen.getByLabelText('Reps');
		await user.clear(reps);
		await user.type(reps, '8');
		await user.type(screen.getByLabelText('Grip or variant'), 'wide');
		await user.click(screen.getByRole('button', { name: 'Save set' }));
		expect(onsave).toHaveBeenCalledExactlyOnceWith({ set_id: 11, reps: 8, variant: 'wide' });
	});

	it('closes without a call when nothing changed', async () => {
		const { onsave, user } = setup();
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		await user.click(screen.getByRole('button', { name: 'Save set' }));
		expect(onsave).not.toHaveBeenCalled();
		expect(screen.queryByRole('button', { name: 'Save set' })).toBeNull();
	});

	it('enables "Weight is" only for a set done one side at a time', async () => {
		const { onsave, user } = setup();
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		const basis = screen.getByLabelText('Weight is');
		expect(basis.matches(':disabled')).toBe(true);
		await user.click(screen.getByLabelText('One side at a time'));
		expect(basis.matches(':disabled')).toBe(false);
		await user.click(screen.getByRole('button', { name: 'Save set' }));
		expect(onsave).toHaveBeenCalledExactlyOnceWith({ set_id: 11, unilateral: true });
	});

	it('shows the cardio fields behind "More"', async () => {
		const { onsave, user } = setup();
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		expect(screen.queryByLabelText('Distance (km)')).toBeNull();
		await user.click(screen.getByRole('button', { name: 'More: cardio fields' }));
		expect(screen.getByLabelText('Average heart rate (bpm)')).toHaveAttribute('step', '1');
		expect(screen.getByLabelText('Calories (kcal)')).toHaveAttribute('step', '1');
		await user.type(screen.getByLabelText('Distance (km)'), '2.5');
		await user.click(screen.getByRole('button', { name: 'Save set' }));
		expect(onsave).toHaveBeenCalledExactlyOnceWith({ set_id: 11, distance_m: 2500 });
	});

	it('opens the cardio fields for a cardio set', async () => {
		const { user } = setup({ ...set, reps: null, weight_kg: null, duration_s: 1800 });
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		expect(screen.getByLabelText('Duration (min)')).toHaveValue(30);
	});

	it('has no edit controls when read-only', () => {
		render(SetRow, { set, editable: false, onsave: vi.fn() });
		expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
		expect(screen.queryByRole('checkbox')).toBeNull();
		expect(screen.getByText('10 × 20 kg')).toBeInTheDocument();
	});
});
