import type { Workout, WorkoutSet } from '#lib/api/results.js';
import type {
	LoadBasis,
	SetAddArgs,
	SetType,
	SetUpdateArgs,
	WorkoutUpdateArgs
} from '#lib/api/tools.gen.js';
import { fromLocalInput, toLocalInput } from '#lib/vitals.js';

export const SET_TYPES: { value: SetType; label: string }[] = [
	{ value: 'working', label: 'Working' },
	{ value: 'warmup', label: 'Warm-up' },
	{ value: 'partial', label: 'Partial' },
	{ value: 'interrupted', label: 'Cut short' }
];

export const LOAD_BASES: { value: LoadBasis; label: string }[] = [
	{ value: 'default', label: 'Usual (the number on the implement)' },
	{ value: 'per_implement', label: 'Per dumbbell or kettlebell' },
	{ value: 'per_side', label: 'Per side' },
	{ value: 'total', label: 'Both sides together' }
];

export const setTypeLabel = (t: SetType) => SET_TYPES.find((x) => x.value === t)?.label ?? t;

/** The workout editor's fields; times are `datetime-local` values, `gym` an id. */
export interface WorkoutForm {
	start: string;
	finish: string;
	gym: string;
	notes: string;
	time_estimated: boolean;
	shift_sets: boolean;
}

export const workoutForm = (w: Workout): WorkoutForm => ({
	start: toLocalInput(w.started_at),
	finish: w.finished_at ? toLocalInput(w.finished_at) : '',
	gym: String(w.gym.id),
	notes: w.notes ?? '',
	time_estimated: w.time_estimated,
	shift_sets: true
});

/**
 * workout_update arguments for what changed. A new start clears "estimated" on the server unless it is sent along,
 * so it is; `shift_sets` only goes with a new start.
 */
export function workoutUpdateArgs(w: Workout, f: WorkoutForm): Omit<WorkoutUpdateArgs, 'owner'> {
	const out: Omit<WorkoutUpdateArgs, 'owner'> = { workout_id: w.workout_id };
	const was = workoutForm(w);
	if (f.start && f.start !== was.start) {
		out.started_at = fromLocalInput(f.start);
		if (f.shift_sets) out.shift_sets = true;
	}
	if (f.finish && f.finish !== was.finish) out.finished_at = fromLocalInput(f.finish);
	if (f.gym !== was.gym) out.gym = f.gym;
	if (f.notes.trim() !== was.notes) out.notes = f.notes.trim();
	if (f.time_estimated !== w.time_estimated || (out.started_at && f.time_estimated))
		out.time_estimated = f.time_estimated;
	return out;
}

/** Kilometres from a form as metres, to the millimetre: `km * 1000` alone can leave float noise (1.001 → 1001.0000000000001). */
const metres = (km: number) => Math.round(km * 1e6) / 1e3;

const num = (n: number, digits = 1) =>
	n.toLocaleString(undefined, { maximumFractionDigits: digits });

/** "10 × 20 kg", "8 per side × 12 kg", "30 min · 5 km": what the set was, in one line. */
export function setSummary(s: WorkoutSet): string {
	const parts: string[] = [];
	if (s.reps != null) {
		const reps = `${s.reps}${s.unilateral ? ' per side' : ''}`;
		parts.push(s.weight_kg != null ? `${reps} × ${num(s.weight_kg, 2)} kg` : `${reps} reps`);
	} else if (s.weight_kg != null) parts.push(`${num(s.weight_kg, 2)} kg`);
	if (s.duration_s != null) parts.push(`${num(s.duration_s / 60)} min`);
	if (s.distance_m != null) parts.push(`${num(s.distance_m / 1000, 2)} km`);
	return parts.join(' · ') || '–';
}

/** The set editor's fields as the inputs hold them; an empty number input binds as null or undefined. */
export interface SetForm {
	reps: number | null;
	weight_kg: number | null;
	set_type: SetType;
	rpe: number | null;
	rest_s: number | null;
	machine: string;
	variant: string;
	unilateral: boolean;
	load_basis: LoadBasis;
	notes: string;
	duration_min: number | null;
	distance_km: number | null;
	avg_hr: number | null;
	max_hr: number | null;
	avg_speed_kmh: number | null;
	avg_incline_pct: number | null;
	calories: number | null;
}

export function setForm(s: WorkoutSet): SetForm {
	return {
		reps: s.reps,
		weight_kg: s.weight_kg,
		set_type: s.set_type,
		rpe: s.rpe,
		rest_s: s.rest_s,
		machine: s.machine ?? '',
		variant: s.variant ?? '',
		unilateral: s.unilateral,
		load_basis: s.load_basis ?? 'default',
		notes: s.notes ?? '',
		duration_min: s.duration_s == null ? null : s.duration_s / 60,
		distance_km: s.distance_m == null ? null : s.distance_m / 1000,
		avg_hr: s.avg_hr,
		max_hr: s.max_hr,
		avg_speed_kmh: s.avg_speed_kmh,
		avg_incline_pct: s.avg_incline_pct,
		calories: s.calories
	};
}

type UpdateArgs = Omit<SetUpdateArgs, 'owner'>;

/**
 * set_update arguments for the fields that differ from `s`: just `set_id` when nothing changed. The server keeps a
 * number it gets null for, so an emptied number field is left alone; an emptied text field sends "" and clears it.
 * `load_basis` only goes with a unilateral set (turning unilateral off clears it on the server).
 */
export function setUpdateArgs(s: WorkoutSet, f: SetForm): UpdateArgs {
	const out: UpdateArgs = { set_id: s.set_id };
	const numbers = [
		['reps', f.reps, s.reps],
		['weight_kg', f.weight_kg, s.weight_kg],
		['rpe', f.rpe, s.rpe],
		['rest_s', f.rest_s, s.rest_s],
		['avg_hr', f.avg_hr, s.avg_hr],
		['max_hr', f.max_hr, s.max_hr],
		['avg_speed', f.avg_speed_kmh, s.avg_speed_kmh],
		['avg_incline_pct', f.avg_incline_pct, s.avg_incline_pct],
		['calories', f.calories, s.calories],
		['duration_s', f.duration_min == null ? null : Math.round(f.duration_min * 60), s.duration_s],
		['distance_m', f.distance_km == null ? null : metres(f.distance_km), s.distance_m]
	] as const;
	for (const [key, value, was] of numbers) if (value != null && value !== was) out[key] = value;
	for (const key of ['machine', 'variant', 'notes'] as const)
		if (f[key].trim() !== (s[key] ?? '')) out[key] = f[key].trim();
	if (f.set_type !== s.set_type) out.set_type = f.set_type;
	if (f.unilateral !== s.unilateral) out.unilateral = f.unilateral;
	if (f.unilateral && f.load_basis !== (s.load_basis ?? 'default')) out.load_basis = f.load_basis;
	return out;
}

/** What "Change selected" can set on several sets at once. */
export const BULK_FIELDS = [
	{ field: 'set_type', label: 'Set type' },
	{ field: 'weight_kg', label: 'Weight (kg)' },
	{ field: 'reps', label: 'Reps' },
	{ field: 'rpe', label: 'RPE' },
	{ field: 'rest_s', label: 'Rest (s)' },
	{ field: 'machine', label: 'Machine' },
	{ field: 'variant', label: 'Variant' }
] as const;
export type BulkField = (typeof BULK_FIELDS)[number]['field'];

/** set_update arguments that set `field` to `value` on every set in `ids`; null when `value` isn't a number it needs. */
export function bulkArgs(ids: number[], field: BulkField, value: string): UpdateArgs | null {
	const base = { set_ids: ids };
	switch (field) {
		case 'set_type':
			return { ...base, set_type: value as SetType };
		case 'machine':
		case 'variant':
			return { ...base, [field]: value.trim() };
		default: {
			const n = Number(value);
			if (value.trim() === '' || !Number.isFinite(n) || n < 0) return null;
			// The server takes whole numbers for these.
			if ((field === 'reps' || field === 'rest_s') && !Number.isInteger(n)) return null;
			return { ...base, [field]: n };
		}
	}
}

/** One row of the "Add sets" form. `time` is a `datetime-local` value, needed only on a finished workout. */
export interface NewSet {
	reps: number | null;
	weight_kg: number | null;
	rpe: number | null;
	duration_min: number | null;
	distance_km: number | null;
	set_type: SetType;
	time: string;
}

export const newSet = (prev?: NewSet): NewSet =>
	prev
		? { ...prev }
		: {
				reps: null,
				weight_kg: null,
				rpe: null,
				duration_min: null,
				distance_km: null,
				set_type: 'working',
				time: ''
			};

const clock = (iso: string) =>
	new Date(iso).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });

/**
 * set_add arguments for `rows` of `exercise` in workout `w`, or the first problem in words. A finished workout takes
 * only sets with a time inside its start and finish (the server refuses the rest).
 */
export function addSetsArgs(
	exercise: number,
	rows: NewSet[],
	w: Workout,
	unilateral: boolean
): { args: Omit<SetAddArgs, 'owner'> } | { error: string } {
	if (rows.length < 1 || rows.length > 50) return { error: 'Add between 1 and 50 sets at a time.' };
	const sets: SetAddArgs['sets'] = [];
	for (const [i, r] of rows.entries()) {
		const n = i + 1;
		if (r.reps == null && r.duration_min == null && r.distance_km == null)
			return { error: `Set ${n}: enter the reps, a duration or a distance.` };
		let performed_at: string | undefined;
		if (w.finished_at) {
			const window = `between ${clock(w.started_at)} and ${clock(w.finished_at)}`;
			if (!r.time)
				return { error: `Set ${n}: this workout is finished, so pick a time ${window}.` };
			performed_at = fromLocalInput(r.time);
			const at = new Date(performed_at).getTime();
			const start = new Date(w.started_at).getTime();
			// The input has whole minutes and the start may have seconds: the start's own minute means the start.
			if (at < start && at > start - 60_000) performed_at = w.started_at;
			else if (at < start || at > new Date(w.finished_at).getTime())
				return { error: `Set ${n}: pick a time ${window}, while the workout ran.` };
		}
		sets.push({
			...(r.reps != null && { reps: r.reps }),
			...(r.weight_kg != null && { weight_kg: r.weight_kg }),
			...(r.rpe != null && { rpe: r.rpe }),
			...(r.duration_min != null && { duration_s: Math.round(r.duration_min * 60) }),
			...(r.distance_km != null && { distance_m: metres(r.distance_km) }),
			...(r.set_type !== 'working' && { set_type: r.set_type }),
			...(performed_at && { performed_at })
		});
	}
	return {
		args: {
			exercise: String(exercise),
			workout_id: w.workout_id,
			sets,
			...(unilateral && { unilateral: true })
		}
	};
}
