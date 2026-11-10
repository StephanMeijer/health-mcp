/** Muscle groups an exercise stores (src/gym/setup.rs), and the coarse names that work only as filters. */
export const MUSCLE_GROUPS = [
	'chest',
	'lats',
	'upper_back',
	'lower_back',
	'front_delts',
	'side_delts',
	'rear_delts',
	'biceps',
	'triceps',
	'forearms',
	'core',
	'glutes',
	'quads',
	'hamstrings',
	'adductors',
	'abductors',
	'calves',
	'neck'
] as const;

export type MuscleGroup = (typeof MUSCLE_GROUPS)[number];

/** Filters (exercise_search, settings priority) expand these; an exercise can't store them. */
export const COARSE_GROUPS: Readonly<Record<string, readonly MuscleGroup[]>> = {
	back: ['lats', 'upper_back', 'lower_back'],
	shoulders: ['front_delts', 'side_delts', 'rear_delts'],
	legs: ['quads', 'hamstrings', 'glutes', 'adductors', 'abductors', 'calves'],
	arms: ['biceps', 'triceps', 'forearms']
};

/** "upper_back" → "Upper back". */
export function muscleLabel(m: string): string {
	const s = m.replaceAll('_', ' ');
	return s.charAt(0).toUpperCase() + s.slice(1);
}
