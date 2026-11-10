import { cleanup, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { kindInfo } from '#lib/kinds.js';
import type { HeartRateRow } from '#lib/api/results.js';
import ReadingForm from './reading-form.svelte';

function setup(kind: string, row: HeartRateRow | null = null) {
	const onadd = vi.fn(async () => {});
	const onupdate = vi.fn(async () => {});
	render(ReadingForm, { kind: kindInfo(kind)!, row, onadd, onupdate, onclose: () => {} });
	return { user: userEvent.setup({ pointerEventsCheck: 0 }), onadd, onupdate };
}

describe('ReadingForm', () => {
	it('logs blood pressure as systolic, diastolic and pulse at one time', async () => {
		const { user, onadd } = setup('diastolic');
		await user.type(screen.getByLabelText('Systolic (mmHg)'), '121');
		await user.type(screen.getByLabelText('Diastolic (mmHg)'), '79');
		await user.type(screen.getByLabelText('Pulse (bpm)'), '62');
		await user.click(screen.getByRole('button', { name: 'Add blood pressure' }));
		const calls = (
			onadd.mock.calls[0] as unknown as [
				{ kind: string; readings: { time: string; value: number }[] }[]
			]
		)[0];
		expect(calls.map((c) => [c.kind, c.readings[0].value])).toEqual([
			['systolic', 121],
			['diastolic', 79],
			['pulse', 62]
		]);
		expect(new Set(calls.map((c) => c.readings[0].time)).size).toBe(1);
	});

	it('leaves pulse out when it is empty', async () => {
		const { user, onadd } = setup('systolic');
		await user.type(screen.getByLabelText('Systolic (mmHg)'), '130');
		await user.type(screen.getByLabelText('Diastolic (mmHg)'), '85');
		await user.click(screen.getByRole('button', { name: 'Add blood pressure' }));
		const calls = (onadd.mock.calls[0] as unknown as [{ kind: string }[]])[0];
		expect(calls.map((c) => c.kind)).toEqual(['systolic', 'diastolic']);
	});

	it("sends a heart rate in the kind's field", async () => {
		const { user, onadd } = setup('heart_rate');
		await user.type(screen.getByLabelText('Heart rate (bpm)'), '58');
		await user.click(screen.getByRole('button', { name: 'Add reading' }));
		expect(onadd).toHaveBeenCalledWith([
			{ kind: 'heart_rate', readings: [expect.objectContaining({ bpm: 58, note: null })] }
		]);
	});

	it('edits a reading by its time', async () => {
		const row: HeartRateRow = {
			time: '2026-11-09T07:00:00+00:00',
			source: 'manual',
			bpm: 60,
			rr_ms: null,
			note: null,
			device_id: null
		};
		const { user, onupdate } = setup('heart_rate', row);
		const input = screen.getByLabelText('Heart rate (bpm)');
		await user.clear(input);
		await user.type(input, '57');
		await user.click(screen.getByRole('button', { name: 'Save reading' }));
		expect(onupdate).toHaveBeenCalledWith(row.time, { bpm: 57, note: '' });
	});

	it('clears the note of a reading with an empty string', async () => {
		const row: HeartRateRow = {
			time: '2026-11-09T07:00:00+00:00',
			source: 'manual',
			bpm: 60,
			rr_ms: null,
			note: 'after a run',
			device_id: null
		};
		const { user, onupdate } = setup('heart_rate', row);
		await user.clear(screen.getByLabelText('Note (optional)'));
		await user.click(screen.getByRole('button', { name: 'Save reading' }));
		expect(onupdate).toHaveBeenCalledWith(row.time, { bpm: 60, note: '' });
	});

	it('takes whole beats per minute for heart rate only', () => {
		setup('heart_rate');
		expect(screen.getByLabelText('Heart rate (bpm)')).toHaveAttribute('step', '1');
		cleanup();
		setup('weight');
		expect(screen.getByLabelText('Weight (kg)')).toHaveAttribute('step', 'any');
	});
});
