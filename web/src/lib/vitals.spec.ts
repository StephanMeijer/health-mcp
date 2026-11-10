import { describe, expect, it, vi } from 'vitest';
import { addAll, type AddCall } from './vitals.js';

const t = '2026-11-09T07:00:00.000Z';
const one = (kind: AddCall['kind'], value: number): AddCall => ({
	kind,
	readings: [{ time: t, value }]
});

describe('addAll', () => {
	it('deletes the readings already added when a later one fails, so a retry starts clean', async () => {
		const failure = new Error('session expired');
		const add = vi.fn(async (c: AddCall) => {
			if (c.kind === 'diastolic') throw failure;
		});
		const remove = vi.fn(async () => {});
		await expect(addAll([one('systolic', 121), one('diastolic', 79)], add, remove)).rejects.toBe(
			failure
		);
		expect(remove).toHaveBeenCalledExactlyOnceWith({ kind: 'systolic', times: [t] });
	});

	it('deletes nothing when every call succeeds', async () => {
		const remove = vi.fn(async () => {});
		await addAll([one('systolic', 121), one('diastolic', 79)], async () => {}, remove);
		expect(remove).not.toHaveBeenCalled();
	});
});
