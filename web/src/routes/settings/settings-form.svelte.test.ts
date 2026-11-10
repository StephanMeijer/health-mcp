import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Settings } from '#lib/api/results.js';
import SettingsForm from './settings-form.svelte';

const mutate = vi.hoisted(() => vi.fn());
vi.mock('#lib/api/query.svelte.js', () => ({
	toolMutation: () => ({ mutate, isPending: false })
}));

const settings: Settings = {
	timezone: 'Europe/Amsterdam',
	now: '2026-11-10T13:00:00Z',
	today: '2026-11-10',
	goal: 'strength',
	goals_notes: 'Hyrox on 14 March',
	priority_muscle_groups: ['chest'],
	limitations: 'Left shoulder: no overhead pressing'
};

describe('SettingsForm', () => {
	it('sends only the changed fields, and an emptied field as "" to clear it', async () => {
		const user = userEvent.setup();
		render(SettingsForm, { settings });
		const save = screen.getByRole('button', { name: 'Save settings' });
		expect(save.matches(':disabled')).toBe(true);
		await user.clear(screen.getByLabelText('Injuries and limitations'));
		await user.click(screen.getByLabelText('Back'));
		await user.click(save);
		expect(mutate).toHaveBeenCalledExactlyOnceWith({
			limitations: '',
			priority_muscle_groups: ['chest', 'back']
		});
	});

	it('keeps edits through a refetch, and shows the saved values after a save', async () => {
		const user = userEvent.setup();
		const { rerender } = render(SettingsForm, { settings });
		const notes = screen.getByLabelText('Goals and plans');
		await user.clear(notes);
		await user.type(notes, 'Marathon in April');
		await rerender({ settings: { ...settings, now: '2026-11-10T14:00:00Z' } });
		expect(screen.getByLabelText('Goals and plans')).toHaveValue('Marathon in April');
		await rerender({
			settings: { ...settings, goals_notes: 'Marathon in April', limitations: 'None' }
		});
		expect(screen.getByLabelText('Goals and plans')).toHaveValue('Marathon in April');
		expect(screen.getByLabelText('Injuries and limitations')).toHaveValue('None');
		expect(screen.getByRole('button', { name: 'Save settings' }).matches(':disabled')).toBe(true);
		await user.type(screen.getByLabelText('Injuries and limitations'), ' yet');
		await user.click(screen.getByRole('button', { name: 'Undo changes' }));
		expect(screen.getByLabelText('Injuries and limitations')).toHaveValue('None');
	});

	it('hides saving and disables the fields on a read-only logbook', () => {
		render(SettingsForm, { settings, readonly: true });
		expect(screen.queryByRole('button', { name: 'Save settings' })).toBeNull();
		expect(screen.getByLabelText('Goals and plans').matches(':disabled')).toBe(true);
	});
});
