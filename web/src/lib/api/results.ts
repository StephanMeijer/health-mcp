/**
 * What the tools return. The server publishes argument schemas only (tools.json), so these are written by hand from
 * the Rust result builders; each type names its source. Timestamps are RFC 3339 strings.
 */

import type { KindInfo } from '#lib/kinds.js';
import type { Kind, LoadBasis, SetType, Target } from './tools.gen.js';

export type Access = 'none' | 'ro' | 'rw';

/** gym_get, gym_create, gym_update (src/gym/setup.rs, gym_json). */
export interface Gym {
	id: number;
	name: string;
	aliases: string[];
	equipment: string | null;
	notes: string | null;
	last_workout_at: string | null;
}

/** gym_list, one row (src/gym/setup.rs, gym_list). */
export type GymListItem = Pick<Gym, 'id' | 'name' | 'aliases' | 'last_workout_at'>;

/** exercise_search (array), exercise_create, exercise_update (src/gym/setup.rs, EXERCISE_JSON). */
export interface Exercise {
	id: number;
	name: string;
	aliases: string[];
	kind: 'strength' | 'cardio';
	description: string | null;
	muscle_groups: string[];
	secondary_muscle_groups: string[];
	equipment: string | null;
}

/** exercise_alternatives (src/gym/search.rs). */
export interface Alternatives {
	gym: { id: number; name: string; equipment: string | null } | null;
	alternatives: {
		id: number;
		name: string;
		muscle_groups: string[];
		shared_muscle_groups: string[];
		shared_secondary: string[];
		equipment: string | null;
	}[];
}

/** One set as every view shows it (src/gym/workout.rs, SET_JSON, plus set_id/set_number/performed_at in workout_json). */
export interface WorkoutSet {
	set_id: number;
	/** 1, 2, … per exercise, in the order performed. */
	set_number: number;
	performed_at: string;
	reps: number | null;
	weight_kg: number | null;
	duration_s: number | null;
	rpe: number | null;
	rest_s: number | null;
	set_type: SetType;
	unilateral: boolean;
	/** Null is the default convention ('default' is stored as null). */
	load_basis: Exclude<LoadBasis, 'default'> | null;
	distance_m: number | null;
	intensity: string | null;
	avg_hr: number | null;
	max_hr: number | null;
	calories: number | null;
	avg_incline_pct: number | null;
	max_incline_pct: number | null;
	avg_speed_kmh: number | null;
	max_speed_kmh: number | null;
	machine: string | null;
	variant: string | null;
	time_estimated: boolean;
	notes: string | null;
}

/** workout_list rows, workout_add_past and workout_update, and inside the others (src/gym/workout.rs, workout_json). */
export interface Workout {
	workout_id: number;
	gym: { id: number; name: string };
	started_at: string;
	/** Null while it is open. */
	finished_at: string | null;
	finish_reason: 'manual' | 'stale' | 'superseded' | null;
	time_estimated: boolean;
	notes: string | null;
	set_count: number;
	/** In the order first trained; sets in the order performed. */
	exercises: { exercise_id: number; exercise: string; sets: WorkoutSet[] }[];
}

/** An open workout closed (or, from workout_current, about to be) for inactivity (src/gym/workout.rs, close_stale). */
export interface ClosedStale {
	workout_id: number;
	gym: string;
	/** "deleted (no sets, no notes)", "auto-finished (stale)", or "stale: will be …" from workout_current. */
	action: string;
	last_activity: string;
	sets: number;
}

/** workout_current (src/gym/workout.rs, current). */
export interface WorkoutCurrent {
	open_workout: Workout | null;
	closed_stale: ClosedStale | null;
}

/** workout_start (src/gym/workout.rs, start). */
export interface WorkoutStartResult {
	status: 'started' | 'resumed';
	workout: Workout;
	superseded?: { workout_id: number; gym: string } | null;
	closed_stale: ClosedStale | null;
}

/** set_add (src/gym/workout.rs, set_add). */
export interface SetAddResult {
	added: number;
	machine: string | null;
	machine_source: 'given' | 'last_used_here' | 'none';
	variant: string | null;
	variant_source: 'given' | 'last_used_here' | 'none';
	workout: Workout;
	started_at_moved_back: boolean;
	closed_stale: ClosedStale | null;
}

/** set_update and set_delete with set_ids (with set_id they return the Workout) (src/gym/workout.rs, touched_json). */
export interface SetBatchResult {
	workouts: Workout[];
}

export type Goal = 'strength' | 'hypertrophy' | 'endurance' | 'general';

/** settings_get, settings_update (src/gym/setup.rs, settings_get). */
export interface Settings {
	timezone: string;
	now: string;
	/** YYYY-MM-DD in `timezone`. */
	today: string;
	goal: Goal | null;
	goals_notes: string | null;
	priority_muscle_groups: string[] | null;
	limitations: string | null;
}

/** body_note_add/update and body_note_list rows (src/gym/body_note.rs, ROW). */
export interface BodyNote {
	id: number;
	noted_at: string;
	location: string;
	severity: number | null;
	notes: string | null;
	workout_id: number | null;
}

/** One comment in a thread (src/gym/comment.rs, THREADS). A deleted one keeps its place with `body` null. */
export interface Comment {
	id: number;
	/** The author's email. */
	author: string;
	by_owner: boolean;
	yours: boolean;
	body: string | null;
	deleted: boolean;
	created_at: string;
	edited_at: string | null;
}

/** comment_list (array), thread_start, comment_reply/update/delete, thread_resolve (src/gym/comment.rs, THREADS). */
export interface Thread {
	thread_id: number;
	target: Target;
	/** Exactly one of these three is set, matching `target`. */
	workout_id: number | null;
	exercise_id: number | null;
	set_id: number | null;
	/** The exercise's name, for an exercise or set thread. */
	exercise: string | null;
	resolved: boolean;
	resolved_at: string | null;
	created_at: string;
	/** You started it or own the logbook: you may resolve or reopen it. */
	can_resolve: boolean;
	/** Oldest first. */
	comments: Comment[];
}

/** share_list (src/share.rs). */
export interface ShareList {
	/** Null when the login carries no verified email: sharing then can't be used. */
	your_email: string | null;
	shared_by_you: { email: string; gym: Access; vitals: Access; since: string }[];
	shared_with_you: { owner: string; gym: Access; vitals: Access; since: string }[];
	invites_sent: { email: string; gym: Access; vitals: Access; expires_at: string }[];
	invites_received: {
		invite_id: number;
		from: string;
		gym: Access;
		vitals: Access;
		expires_at: string;
	}[];
}

/** share_invite (src/share.rs). */
export interface ShareInviteResult {
	invited: string;
	gym: Access;
	vitals: Access;
	expires_at: string;
	/** False without a mail server: pass `link` on by hand. */
	mailed: boolean;
	link: string;
}

export type Source = 'manual' | 'app' | 'import';

/** reading_list rows: whole table rows without user_id (src/tools.rs reading_list; columns from the migrations). */
export interface HeartRateRow {
	time: string;
	source: Source;
	bpm: number;
	/** Beat-to-beat intervals from a chest strap. */
	rr_ms: number[] | null;
	note: string | null;
	device_id: number | null;
}
export interface HrvRow {
	time: string;
	source: Source;
	rmssd_ms: number;
	note: string | null;
}
export interface MeasurementRow {
	time: string;
	source: Source;
	device_id: number | null;
	type: string;
	value: number;
	unit: string;
	note: string | null;
}
export type ReadingRow = HeartRateRow | HrvRow | MeasurementRow;

export type Stage = 'awake' | 'light' | 'deep' | 'rem';

/** sleep_list, one session (src/tools.rs sleep_list). */
export interface SleepSession {
	id: string;
	source: Source;
	start_at: string;
	end_at: string;
	note: string | null;
	hours: number;
	stage_seconds: Partial<Record<Stage, number>> | null;
	stages: { time: string; stage: Stage; duration_s: number }[] | null;
}

/** sleep_list (src/tools.rs sleep_list, capped_json): `truncated` means the list was cut; narrow the range. */
export type SleepList = { truncated: boolean; sessions: SleepSession[] };

/** The sessions of a sleep_list result; also reads a bare array, the shape before the cap. */
export function sessions(result: unknown): SleepSession[] {
	if (Array.isArray(result)) return result as SleepSession[];
	return (result as SleepList | null | undefined)?.sessions ?? [];
}

export type Stats = { avg_7d: number | null; avg_30d: number | null; n_30d: number };

/** summary (src/tools.rs summary): per reading kind its latest row and stats of its field (Kind::fields), plus sleep. */
export type Summary = Record<
	Kind,
	{ latest: ReadingRow | null } & Partial<Record<KindInfo['field'], Stats>>
> & {
	sleep: {
		avg_hours_7d: number | null;
		avg_hours_30d: number | null;
		nights_30d: number;
		stage_pct_30d: Partial<Record<Stage, number>> | null;
	};
};

/** progress (src/gym/search.rs, progress). With an exercise, one row per bucket and machine/variant setup. */
export interface Progress {
	bucket: 'day' | 'week' | 'month';
	series: { period_start: string; machine: string | null; variant: string | null; value: number }[];
}

type SetRecord = {
	weight_kg: number;
	reps: number;
	performed_at: string;
	set_id: number;
	workout_id: number;
};

/** personal_records, one row per exercise and setup (src/gym/search.rs, personal_records). Cardio records left out. */
export interface PersonalRecord {
	exercise: string;
	machine: string | null;
	variant: string | null;
	heaviest_set: SetRecord | null;
	best_e1rm: (SetRecord & { e1rm_kg: number }) | null;
	max_reps_by_weight_kg: { weight_kg: number; reps: number }[] | null;
	best_volume_workout: { volume_kg: number; workout_id: number; date: string } | null;
}

/** muscle_group_volume (src/gym/search.rs, muscle_volume): weighted sets, primary 1 and secondary 0.5. */
export interface MuscleGroupVolume {
	from: string;
	to: string;
	days: number;
	muscle_groups: {
		muscle_group: string;
		sets: number;
		reps: number;
		sets_per_week: number;
		exercises: number;
		train_days: number;
		avg_rpe: number | null;
		days_since: number | null;
	}[];
}

/** sql_query (src/tools.rs): one object per row, at most 1000. */
export interface SqlRows<T> {
	truncated: boolean;
	rows: T[];
}
