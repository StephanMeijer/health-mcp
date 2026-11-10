import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { SleepSession } from '#lib/api/results.js';
import SleepForm from './sleep-form.svelte';

const night: SleepSession = {
	id: 'a',
	source: 'manual',
	start_at: '2026-11-08T23:00:00Z',
	end_at: '2026-11-09T07:00:00Z',
	note: 'late dinner',
	hours: 8,
	stage_seconds: null,
	stages: null
};

describe('SleepForm', () => {
	it('clears the note of a night with an empty string', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		const onsave = vi.fn(async () => {});
		render(SleepForm, { session: night, onsave, onclose: () => {} });
		await user.clear(screen.getByLabelText('Note (optional)'));
		await user.click(screen.getByRole('button', { name: 'Save sleep' }));
		expect(onsave).toHaveBeenCalledWith(expect.objectContaining({ note: '' }));
	});

	it('logs a night without a note as null', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		const onsave = vi.fn(async () => {});
		render(SleepForm, { onsave, onclose: () => {} });
		await user.click(screen.getByRole('button', { name: 'Log sleep' }));
		expect(onsave).toHaveBeenCalledWith(expect.objectContaining({ note: null }));
	});
});
