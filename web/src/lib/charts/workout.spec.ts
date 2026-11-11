import { describe, expect, it } from 'vitest';
import type { Workout, WorkoutSet } from '#lib/api/results.js';
import { set, workout } from '#lib/test/workouts.js';
import {
	blocks,
	chipText,
	exercisesSql,
	firstTimers,
	heartRateSql,
	hrLine,
	regionOf,
	regionShares,
	summary,
	ticks,
	zoneMinutes,
	type ExerciseInfo
} from './workout.js';

const at = (min: number) =>
	new Date(Date.parse('2026-11-10T18:00:00Z') + min * 60_000).toISOString();
const s = (id: number, min: number, more: Partial<WorkoutSet> = {}): WorkoutSet => ({
	...set,
	set_id: id,
	performed_at: at(min),
	...more
});
const info = (
	id: number,
	kind: ExerciseInfo['kind'],
	muscle_groups: string[],
	secondary_muscle_groups: string[] = [],
	first_at: number | null = null
): ExerciseInfo => ({ id, kind, muscle_groups, secondary_muscle_groups, first_at });

// Lunges, then a row, then lunges again: two lunge blocks.
const w: Workout = {
	...workout,
	exercises: [
		{
			exercise_id: 1,
			exercise: 'Reverse lunge',
			sets: [
				s(1, 5, { reps: 20, weight_kg: 20 }),
				s(2, 7, { reps: 20, weight_kg: 20 }),
				s(3, 40, { reps: 10, weight_kg: 10, set_type: 'warmup' })
			]
		},
		{
			exercise_id: 2,
			exercise: 'Rowing',
			sets: [s(4, 20, { reps: null, weight_kg: null, distance_m: 261, duration_s: 64 })]
		},
		{
			exercise_id: 3,
			exercise: 'Assisted pull-up',
			sets: [s(5, 30, { reps: 8, weight_kg: 30 })]
		},
		{
			exercise_id: 4,
			exercise: 'Sled pull',
			sets: [s(6, 50, { reps: null, weight_kg: 115, distance_m: 18 })]
		}
	]
};
const infos = new Map(
	[
		info(1, 'strength', ['quads', 'glutes'], ['hamstrings']),
		info(2, 'cardio', ['quads'], ['lats']),
		info(3, 'strength', ['lats'], ['biceps']),
		info(4, 'strength', ['lats', 'upper_back'])
	].map((i) => [i.id, i])
);

describe('summary', () => {
	it('counts working strength sets, volume without warm-ups or assisted machines, and cardio apart', () => {
		expect(summary(w, infos)).toMatchObject({
			minutes: 60,
			sets: 4,
			exercises: 4,
			volumeKg: 800,
			carriedM: 18,
			cardioM: 261,
			cardioMin: 1,
			machines: ['Rowing']
		});
	});

	it('counts a unilateral set per side and splits a total load over both sides', () => {
		const uni: Workout = {
			...workout,
			exercises: [
				{
					exercise_id: 1,
					exercise: 'Split squat',
					sets: [s(1, 5, { reps: 10, weight_kg: 40, unilateral: true, load_basis: 'total' })]
				}
			]
		};
		expect(summary(uni, infos).volumeKg).toBe(400);
	});
});

describe('regionOf', () => {
	it('takes cardio by kind, else the region of the main muscle', () => {
		expect(regionOf(infos.get(2))).toBe('Cardio');
		expect(regionOf(infos.get(1))).toBe('Legs');
		expect(regionOf(info(9, 'strength', ['forearms', 'upper_back']))).toBe('Arms');
		expect(regionOf(undefined)).toBeNull();
	});
});

describe('blocks', () => {
	it('groups consecutive sets of one exercise and gives a lone set a minute', () => {
		expect(blocks(w).map((b) => [b.exercise, (b.to - b.from) / 60_000, b.sets])).toEqual([
			['Reverse lunge', 2, 2],
			['Rowing', 1, 1],
			['Assisted pull-up', 1, 1],
			['Reverse lunge', 1, 1],
			['Sled pull', 1, 1]
		]);
		expect(blocks(w)[1].to).toBe(Date.parse(at(20)));
	});

	it('has a tick per set in time order', () => {
		expect(ticks(w).map((t) => t.setId)).toEqual([1, 2, 4, 5, 3, 6]);
	});
});

describe('zoneMinutes', () => {
	it('puts each minute in its band, edges going up', () => {
		expect(zoneMinutes([99, 99.6, 100, 119, 120, 139.4, 140, 159, 160, 190])).toEqual([
			1, 3, 2, 2, 2
		]);
	});
});

describe('regionShares', () => {
	it('weighs main muscles 1 and supporting ½, working strength sets only', () => {
		// Lunges: 2 sets × (quads + glutes) = 4 legs + 2 × ½ hamstrings = 5; pull-up 1 back + ½ arms; sled 2 back.
		expect(regionShares(w, infos)).toEqual([
			{ region: 'Back', sets: 3, pct: 35 },
			{ region: 'Arms', sets: 0.5, pct: 6 },
			{ region: 'Legs', sets: 5, pct: 59 }
		]);
	});
});

describe('chipText', () => {
	it('reads like the report', () => {
		expect(chipText(s(1, 0, { reps: 20, weight_kg: 20 }))).toBe('20 × 20');
		expect(chipText(s(1, 0, { reps: 8, weight_kg: 12.5, unilateral: true }))).toBe('8/side × 12.5');
		expect(chipText(s(1, 0, { reps: 12, weight_kg: null }))).toBe('12 reps');
		expect(chipText(s(1, 0, { reps: null, weight_kg: 24, distance_m: 50 }))).toBe('50 m × 24');
		expect(
			chipText(s(1, 0, { reps: null, weight_kg: null, distance_m: 261, duration_s: 64 }))
		).toBe('261 m · 1:04');
		expect(chipText(s(1, 0, { reps: null, weight_kg: null, duration_s: 90 }))).toBe('1:30');
	});
});

describe('firstTimers', () => {
	it('marks exercises whose first set ever is in this workout', () => {
		const start = Date.parse(workout.started_at) / 1000;
		const list = [
			info(1, 'strength', [], [], start + 300),
			info(2, 'strength', [], [], start - 86_400),
			info(3, 'strength', [], [], null)
		];
		expect([...firstTimers(w, list)]).toEqual([1]);
	});
});

describe('sql', () => {
	it('holds only numbers', () => {
		expect(exercisesSql([3, 7])).toContain('e.id IN (3, 7)');
		expect(exercisesSql([])).toContain('IN (NULL)');
		expect(heartRateSql(1_000_500, 2_000_000)).toContain(
			"time_bucket('1 minute'::interval, time, to_timestamp(1000))"
		);
		expect(heartRateSql(1_000_500, 2_000_000)).toContain('time < to_timestamp(2000)');
	});
});

describe('hrLine', () => {
	it('rounds to whole bpm and breaks the line over a missing minute', () => {
		expect(
			hrLine([
				{ t: 0, bpm: 120.4 },
				{ t: 60, bpm: 130.6 },
				{ t: 240, bpm: 140 }
			])
		).toEqual([
			[0, 120],
			[60_000, 131],
			[210_000, null],
			[240_000, 140]
		]);
	});
});
