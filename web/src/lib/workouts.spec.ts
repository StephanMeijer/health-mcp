import { describe, expect, it } from 'vitest';
import { set, workout } from '#lib/test/workouts.js';
import { toLocalInput } from '#lib/vitals.js';
import {
	addSetsArgs,
	bulkArgs,
	justAfter,
	newSet,
	setForm,
	setUpdateArgs,
	workoutForm,
	workoutUpdateArgs
} from './workouts.js';

describe('justAfter', () => {
	it('is one millisecond past the start, microseconds dropped', () => {
		expect(justAfter('2026-11-10T18:00:00.123456+00:00')).toBe('2026-11-10T18:00:00.124Z');
	});
});

describe('setUpdateArgs', () => {
	it('sends only the id when nothing changed', () => {
		expect(setUpdateArgs(set, setForm(set))).toEqual({ set_id: 11 });
	});

	it('sends the changed fields, clears text with "" and keeps emptied numbers', () => {
		const f = { ...setForm(set), reps: 8, rpe: null, machine: ' ', duration_min: 1.5 };
		expect(setUpdateArgs(set, f)).toEqual({ set_id: 11, reps: 8, machine: '', duration_s: 90 });
	});

	it('sends load_basis only for a unilateral set', () => {
		expect(setUpdateArgs(set, { ...setForm(set), load_basis: 'per_side' })).toEqual({ set_id: 11 });
		expect(
			setUpdateArgs(set, { ...setForm(set), unilateral: true, load_basis: 'per_side' })
		).toEqual({ set_id: 11, unilateral: true, load_basis: 'per_side' });
		const uni = { ...set, unilateral: true, load_basis: 'total' as const };
		expect(setUpdateArgs(uni, { ...setForm(uni), unilateral: false })).toEqual({
			set_id: 11,
			unilateral: false
		});
	});

	it('maps the cardio fields to their arguments', () => {
		const f = { ...setForm(set), distance_km: 5, avg_speed_kmh: 10, avg_hr: 140 };
		expect(setUpdateArgs(set, f)).toEqual({
			set_id: 11,
			distance_m: 5000,
			avg_speed: 10,
			avg_hr: 140
		});
	});
});

describe('bulkArgs', () => {
	it('sets one field on every selected set', () => {
		expect(bulkArgs([1, 2], 'weight_kg', '22.5')).toEqual({ set_ids: [1, 2], weight_kg: 22.5 });
		expect(bulkArgs([1], 'set_type', 'warmup')).toEqual({ set_ids: [1], set_type: 'warmup' });
		expect(bulkArgs([1], 'variant', ' ')).toEqual({ set_ids: [1], variant: '' });
	});

	it('refuses a number field without a number', () => {
		expect(bulkArgs([1], 'reps', '')).toBeNull();
		expect(bulkArgs([1], 'rpe', 'abc')).toBeNull();
	});
});

describe('addSetsArgs', () => {
	const row = { ...newSet(), reps: 10, weight_kg: 20 };

	it('builds the sets for an open workout without times', () => {
		const open = { ...workout, finished_at: null };
		expect(addSetsArgs(7, [row, { ...row, set_type: 'warmup' }], open, true)).toEqual({
			args: {
				exercise: '7',
				workout_id: 4,
				unilateral: true,
				sets: [
					{ reps: 10, weight_kg: 20 },
					{ reps: 10, weight_kg: 20, set_type: 'warmup' }
				]
			}
		});
	});

	it('needs reps, a duration or a distance', () => {
		expect(addSetsArgs(7, [row, newSet()], { ...workout, finished_at: null }, false)).toEqual({
			error: expect.stringMatching(/^Set 2:/)
		});
	});

	it('needs a time inside a finished workout', () => {
		expect(addSetsArgs(7, [row], workout, false)).toEqual({
			error: expect.stringMatching(/Set 1/)
		});
		const late = { ...row, time: toLocalInput('2026-11-10T19:30:00Z') };
		expect(addSetsArgs(7, [late], workout, false)).toEqual({
			error: expect.stringMatching(/while the workout ran/)
		});
		const ok = { ...row, time: toLocalInput('2026-11-10T18:30:00Z') };
		expect(addSetsArgs(7, [ok], workout, false)).toEqual({
			args: {
				exercise: '7',
				workout_id: 4,
				sets: [{ reps: 10, weight_kg: 20, performed_at: '2026-11-10T18:30:00.000Z' }]
			}
		});
	});
});

describe('workoutUpdateArgs', () => {
	it('sends only the id when nothing changed', () => {
		expect(workoutUpdateArgs(workout, workoutForm(workout))).toEqual({ workout_id: 4 });
	});

	it('moves the sets along with a new start, and keeps it estimated when asked', () => {
		const start = toLocalInput('2026-11-10T17:00:00Z');
		expect(workoutUpdateArgs(workout, { ...workoutForm(workout), start })).toEqual({
			workout_id: 4,
			started_at: '2026-11-10T17:00:00.000Z',
			shift_sets: true
		});
		expect(
			workoutUpdateArgs(
				{ ...workout, time_estimated: true },
				{ ...workoutForm(workout), start, shift_sets: false, time_estimated: true }
			)
		).toEqual({ workout_id: 4, started_at: '2026-11-10T17:00:00.000Z', time_estimated: true });
	});

	it('sends a new gym and cleared notes', () => {
		const w = { ...workout, notes: 'legs' };
		expect(workoutUpdateArgs(w, { ...workoutForm(w), gym: '2', notes: '' })).toEqual({
			workout_id: 4,
			gym: '2',
			notes: ''
		});
	});
});
