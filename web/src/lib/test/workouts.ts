import type { Workout, WorkoutSet } from '#lib/api/results.js';

/** Test fixtures: a strength set, and a finished workout without sets. */
export const set: WorkoutSet = {
	set_id: 11,
	set_number: 1,
	performed_at: '2026-11-10T18:10:00Z',
	reps: 10,
	weight_kg: 20,
	duration_s: null,
	rpe: 8,
	rest_s: null,
	set_type: 'working',
	unilateral: false,
	load_basis: null,
	distance_m: null,
	intensity: null,
	avg_hr: null,
	max_hr: null,
	calories: null,
	avg_incline_pct: null,
	max_incline_pct: null,
	avg_speed_kmh: null,
	max_speed_kmh: null,
	machine: 'Technogym',
	variant: null,
	time_estimated: false,
	notes: null
};

export const workout: Workout = {
	workout_id: 4,
	gym: { id: 1, name: 'Gym A' },
	started_at: '2026-11-10T18:00:00Z',
	finished_at: '2026-11-10T19:00:00Z',
	finish_reason: 'manual',
	time_estimated: false,
	notes: null,
	set_count: 0,
	exercises: []
};
