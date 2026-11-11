import type { Goal, Settings } from '#lib/api/results.js';
import type { SettingsUpdateArgs } from '#lib/api/tools.gen.js';

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
