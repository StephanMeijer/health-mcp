import type { Workout, WorkoutSet } from '#lib/api/results.js';
import type { MuscleGroup } from '#lib/muscles.js';
import { REGIONS } from './volume.js';

/** One exercise of the workout as the gym `exercise` view has it, plus its first set ever (epoch seconds). */
export interface ExerciseInfo {
	id: number;
	kind: 'strength' | 'cardio';
	muscle_groups: string[];
	secondary_muscle_groups: string[];
	first_at: number | null;
}

export const CARDIO = 'Cardio';

/** A theme token per region, close to the training report's colours (back blue, legs amber, cardio green). */
export const REGION_COLOR: Record<string, string> = {
	Chest: 'chart-4',
	Back: 'chart-3',
	Shoulders: 'chart-6',
	Arms: 'chart-7',
	Legs: 'chart-2',
	Core: 'chart-5',
	Neck: 'chart-6',
	[CARDIO]: 'chart-1'
};
/** Regions in legend order, Cardio last. */
export const REGION_NAMES = [...REGIONS.map((r) => r.name), CARDIO];

const regionOfMuscle = (m: string) =>
	REGIONS.find((r) => r.muscles.includes(m as MuscleGroup))?.name;

/** Where an exercise goes: cardio by its kind, otherwise the region of its first (main) muscle. */
export const regionOf = (info: ExerciseInfo | undefined) =>
	!info ? null : info.kind === 'cardio' ? CARDIO : (regionOfMuscle(info.muscle_groups[0]) ?? null);

const ms = (iso: string) => Date.parse(iso);
const working = (s: WorkoutSet) => s.set_type === 'working';
// shortcut: assisted machines are known by name only ("Assisted pull-up"); add an exercise flag if names miss some.
const assisted = (name: string) => /assist/i.test(name);
const sides = (s: WorkoutSet) => (s.unilateral ? 2 : 1);

/** The workout's time window in ms: its start (or first set) to its finish (or last set, or now while open). */
export function bounds(w: Workout, now = Date.now()): [number, number] {
	const times = w.exercises.flatMap((e) => e.sets.map((s) => ms(s.performed_at)));
	const end = w.finished_at ? ms(w.finished_at) : now;
	return [Math.min(ms(w.started_at), ...times), Math.max(end, ...times)];
}

/** The numbers of the tiles. Volume is reps × kg of working strength sets as the server counts it, assisted machines left out. */
export function summary(w: Workout, info: Map<number, ExerciseInfo>, now = Date.now()) {
	const [from, to] = bounds(w, now);
	let sets = 0;
	let volumeKg = 0;
	let carriedM = 0;
	let cardioM = 0;
	let cardioS = 0;
	const machines: string[] = [];
	for (const ex of w.exercises) {
		const cardio = info.get(ex.exercise_id)?.kind === 'cardio';
		for (const s of ex.sets.filter(working)) {
			if (cardio) {
				cardioM += s.distance_m ?? 0;
				cardioS += s.duration_s ?? 0;
				if (!machines.includes(ex.exercise)) machines.push(ex.exercise);
				continue;
			}
			sets++;
			carriedM += s.distance_m ?? 0;
			if (s.reps != null && s.weight_kg != null && !assisted(ex.exercise)) {
				const perSide = s.unilateral && s.load_basis === 'total' ? 2 : 1;
				volumeKg += (s.weight_kg / perSide) * s.reps * sides(s);
			}
		}
	}
	return {
		from,
		to,
		minutes: Math.round((to - from) / 60_000),
		sets,
		exercises: w.exercises.length,
		volumeKg,
		carriedM,
		cardioM,
		cardioMin: Math.round(cardioS / 60),
		machines
	};
}

type Timed = { at: number; exerciseId: number; exercise: string; set: WorkoutSet };
const timeline = (w: Workout): Timed[] =>
	w.exercises
		.flatMap((e) =>
			e.sets.map((set) => ({
				at: ms(set.performed_at),
				exerciseId: e.exercise_id,
				exercise: e.exercise,
				set
			}))
		)
		.sort((a, b) => a.at - b.at);

export interface Block {
	exerciseId: number;
	exercise: string;
	from: number;
	to: number;
	sets: number;
}

/**
 * One block per run of consecutive sets of one exercise. A set's time is when it was logged, its end: a block
 * reaches back at least `min` ms before its last set, so a single set still shows.
 */
export function blocks(w: Workout, min = 60_000): Block[] {
	const out: Block[] = [];
	for (const t of timeline(w)) {
		const last = out.at(-1);
		if (last?.exerciseId === t.exerciseId) {
			last.to = t.at;
			last.sets++;
		} else
			out.push({ exerciseId: t.exerciseId, exercise: t.exercise, from: t.at, to: t.at, sets: 1 });
	}
	for (const b of out) b.from = Math.min(b.from, b.to - min);
	return out;
}

/** A tick per logged set: when, and which exercise. */
export const ticks = (w: Workout) =>
	timeline(w).map((t) => ({ at: t.at, exerciseId: t.exerciseId, setId: t.set.set_id }));

/** The training report's fixed heart-rate bands; each holds bpm from `min` up to the next band. */
export const ZONES = [
	{ label: '<100', min: 0 },
	{ label: '100–119', min: 100 },
	{ label: '120–139', min: 120 },
	{ label: '140–159', min: 140 },
	{ label: '160+', min: 160 }
];

/** Minutes per zone from per-minute averages (one row is one minute), rounded as the labels read. */
export function zoneMinutes(bpms: number[]) {
	const minutes = ZONES.map(() => 0);
	for (const b of bpms) minutes[ZONES.findLastIndex((z) => Math.round(b) >= z.min)]++;
	return minutes;
}

/**
 * Weighted working sets per region, as muscle_group_volume counts them: 1 per main muscle, ½ per supporting
 * muscle, per side for unilateral sets. Cardio has no muscles there, so it is left out.
 */
export function regionShares(w: Workout, info: Map<number, ExerciseInfo>) {
	const by = new Map<string, number>();
	for (const ex of w.exercises) {
		const i = info.get(ex.exercise_id);
		if (!i || i.kind === 'cardio') continue;
		const n = ex.sets.filter(working).reduce((s, x) => s + sides(x), 0);
		const add = (m: string, weight: number) => {
			const r = regionOfMuscle(m);
			if (r) by.set(r, (by.get(r) ?? 0) + n * weight);
		};
		i.muscle_groups.forEach((m) => add(m, 1));
		i.secondary_muscle_groups.forEach((m) => add(m, 0.5));
	}
	const total = [...by.values()].reduce((a, b) => a + b, 0);
	return REGION_NAMES.filter((r) => by.get(r)).map((region) => ({
		region,
		sets: by.get(region) as number,
		pct: Math.round(((by.get(region) as number) / total) * 100)
	}));
}

const clockOf = (s: number) =>
	`${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`;
const kg = (n: number) => n.toLocaleString(undefined, { maximumFractionDigits: 2 });

/** A chip's text: "10 × 20", "50 m × 24", "261 m · 1:04"; reps per side get a "/side". */
export function chipText(s: WorkoutSet): string {
	if (s.distance_m != null) {
		const d = `${kg(s.distance_m)} m`;
		if (s.weight_kg != null) return `${d} × ${kg(s.weight_kg)}`;
		return s.duration_s != null ? `${d} · ${clockOf(s.duration_s)}` : d;
	}
	if (s.reps != null) {
		const reps = `${s.reps}${s.unilateral ? '/side' : ''}`;
		return s.weight_kg != null ? `${reps} × ${kg(s.weight_kg)}` : `${reps} reps`;
	}
	if (s.duration_s != null) return clockOf(s.duration_s);
	return s.weight_kg != null ? `${kg(s.weight_kg)} kg` : '–';
}

/** Exercises whose first set ever falls inside this workout. */
export function firstTimers(w: Workout, info: ExerciseInfo[], now = Date.now()) {
	const [from, to] = bounds(w, now);
	return new Set(
		info
			.filter((i) => i.first_at != null && i.first_at * 1000 >= from && i.first_at * 1000 <= to)
			.map((i) => i.id)
	);
}

/** The gym `exercise` view for these ids, with each one's first set. Only numbers go into the SQL. */
export const exercisesSql = (ids: number[]) =>
	`SELECT e.id, e.kind, e.muscle_groups, e.secondary_muscle_groups,
  (SELECT extract(epoch FROM min(s.performed_at))::float8 FROM workout_set s WHERE s.exercise_id = e.id) AS first_at
FROM exercise e WHERE e.id IN (${ids.map((i) => Math.trunc(i)).join(', ') || 'NULL'})`;

/** Average heart rate per minute between two epoch ms, buckets starting at `from`; at most ~1000 rows (16 hours). */
export function heartRateSql(from: number, to: number) {
	const [f, t] = [Math.floor(from / 1000), Math.ceil(to / 1000)];
	return `SELECT extract(epoch FROM time_bucket('1 minute'::interval, time, to_timestamp(${f})))::float8 AS t, avg(bpm)::float8 AS bpm
FROM heart_rate WHERE time >= to_timestamp(${f}) AND time < to_timestamp(${t}) GROUP BY 1 ORDER BY 1`;
}

/** Per-minute rows → `[ms, bpm]` points; a missing minute (strap off) gets a null point so the line breaks there. */
export function hrLine(rows: { t: number; bpm: number }[]) {
	const out: [number, number | null][] = [];
	rows.forEach((r, i) => {
		if (i && r.t - rows[i - 1].t > 90) out.push([(r.t - 30) * 1000, null]);
		out.push([r.t * 1000, Math.round(r.bpm)]);
	});
	return out;
}
