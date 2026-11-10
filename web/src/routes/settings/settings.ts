import type { BodyNote, Goal, Settings } from '#lib/api/results.js';
import type { BodyNoteAddArgs, SettingsUpdateArgs } from '#lib/api/tools.gen.js';

export const GOALS: { value: Goal; label: string }[] = [
	{ value: 'strength', label: 'Strength' },
	{ value: 'hypertrophy', label: 'Muscle size (hypertrophy)' },
	{ value: 'endurance', label: 'Endurance' },
	{ value: 'general', label: 'General fitness' }
];

/** The settings form's fields, as the inputs hold them. */
export interface SettingsForm {
	timezone: string;
	goal: Goal | '';
	goals_notes: string;
	priority_muscle_groups: string[];
	limitations: string;
}

export function settingsForm(s: Settings): SettingsForm {
	return {
		timezone: s.timezone,
		goal: s.goal ?? '',
		goals_notes: s.goals_notes ?? '',
		priority_muscle_groups: [...(s.priority_muscle_groups ?? [])],
		limitations: s.limitations ?? ''
	};
}

const sameSet = (a: string[], b: string[]) =>
	a.length === b.length && a.every((x) => b.includes(x));

/**
 * settings_update arguments for the fields that differ from `s`; empty if nothing changed. An emptied text field
 * sends "" and no muscle groups send [], which the server reads as "clear". A goal can't be cleared.
 */
export function changedSettings(s: Settings, f: SettingsForm): Omit<SettingsUpdateArgs, 'owner'> {
	const out: Omit<SettingsUpdateArgs, 'owner'> = {};
	if (f.timezone !== s.timezone) out.timezone = f.timezone;
	if (f.goal && f.goal !== s.goal) out.goal = f.goal;
	if (f.goals_notes.trim() !== (s.goals_notes ?? '')) out.goals_notes = f.goals_notes.trim();
	if (!sameSet(f.priority_muscle_groups, s.priority_muscle_groups ?? []))
		out.priority_muscle_groups = f.priority_muscle_groups;
	if (f.limitations.trim() !== (s.limitations ?? '')) out.limitations = f.limitations.trim();
	return out;
}

/** The body note form's fields. `noted_at` is a `datetime-local` value in the browser's timezone. */
export interface NoteForm {
	location: string;
	severity: number | null;
	noted_at: string;
	notes: string;
}

/** An RFC 3339 time as a `datetime-local` value in the browser's timezone. */
export function localInput(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function noteForm(n?: BodyNote): NoteForm {
	return {
		location: n?.location ?? '',
		severity: n?.severity ?? null,
		noted_at: n ? localInput(n.noted_at) : '',
		notes: n?.notes ?? ''
	};
}

// An empty number input binds as null or undefined.
const severityOf = (f: NoteForm) => (f.severity == null ? null : f.severity);

/** body_note_add arguments; an empty time means now. */
export function addNoteArgs(f: NoteForm): Omit<BodyNoteAddArgs, 'owner'> {
	return {
		location: f.location.trim(),
		...(severityOf(f) !== null && { severity: severityOf(f) }),
		...(f.noted_at && { noted_at: new Date(f.noted_at).toISOString() }),
		...(f.notes.trim() && { notes: f.notes.trim() })
	};
}

/** body_note_update arguments for what changed; an emptied severity sends 0, which clears it. */
export function updateNoteArgs(n: BodyNote, f: NoteForm) {
	const out: {
		id: number;
		location?: string;
		severity?: number;
		noted_at?: string;
		notes?: string;
	} = { id: n.id };
	if (f.location.trim() !== n.location) out.location = f.location.trim();
	if (severityOf(f) !== n.severity) out.severity = severityOf(f) ?? 0;
	if (f.noted_at && f.noted_at !== localInput(n.noted_at))
		out.noted_at = new Date(f.noted_at).toISOString();
	if (f.notes.trim() !== (n.notes ?? '')) out.notes = f.notes.trim();
	return out;
}
