import { COARSE_GROUPS, MUSCLE_GROUPS, type MuscleGroup } from '#lib/muscles.js';

/** Regions the weekly chart stacks: 18 muscles in one bar are unreadable. */
export const REGIONS: { name: string; muscles: readonly MuscleGroup[] }[] = [
	{ name: 'Chest', muscles: ['chest'] },
	{ name: 'Back', muscles: COARSE_GROUPS.back },
	{ name: 'Shoulders', muscles: COARSE_GROUPS.shoulders },
	{ name: 'Arms', muscles: COARSE_GROUPS.arms },
	{ name: 'Legs', muscles: COARSE_GROUPS.legs },
	{ name: 'Core', muscles: ['core'] },
	{ name: 'Neck', muscles: ['neck'] }
];

/** A local date as `YYYY-MM-DD`, the form the gym tools read as a local day. */
export const isoDay = (d: Date) =>
	`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

/** The Mondays of the last `n` weeks, oldest first; the last one starts this week. */
export function weekStarts(n: number, today = new Date()) {
	const monday = new Date(today.getFullYear(), today.getMonth(), today.getDate());
	monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
	return Array.from({ length: n }, (_, i) => {
		const d = new Date(monday);
		d.setDate(d.getDate() - 7 * (n - 1 - i));
		return d;
	});
}

/** Sets per muscle per week → one stacked series per region that has any sets. */
export function regionSeries(weeks: { muscle_group: string; sets: number }[][]) {
	return REGIONS.map((r) => ({
		name: r.name,
		data: weeks.map((w) =>
			w
				.filter((m) => r.muscles.includes(m.muscle_group as MuscleGroup))
				.reduce((s, m) => s + m.sets, 0)
		)
	})).filter((s) => s.data.some((v) => v > 0));
}

/**
 * Weighted sets in the last 7 days → a shade 0-4 on the body map: 0 none, 1 under 5, 2 under 10,
 * 3 in the 10-20 a week that suits hypertrophy, 4 above it.
 */
export const muscleShade = (sets: number) =>
	sets <= 0 ? 0 : sets < 5 ? 1 : sets < 10 ? 2 : sets <= 20 ? 3 : 4;

// Words people use for a muscle, stems so plurals match too ("hamstring" finds "hamstrings").
const WORDS: [MuscleGroup, string[]][] = [
	['chest', ['chest', 'pec']],
	['lats', ['lat ', 'lats', 'latissimus']],
	['upper_back', ['upper back', 'trap', 'rhomboid']],
	['lower_back', ['lower back', 'lumbar']],
	['front_delts', ['front delt', 'front shoulder']],
	['side_delts', ['side delt', 'side shoulder']],
	['rear_delts', ['rear delt', 'rear shoulder', 'back of shoulder']],
	['biceps', ['bicep']],
	['triceps', ['tricep']],
	['forearms', ['forearm', 'wrist']],
	['core', ['core', 'abs', 'abdom', 'oblique', 'stomach']],
	['glutes', ['glute', 'butt']],
	['quads', ['quad', 'thigh']],
	['hamstrings', ['hamstring']],
	['adductors', ['adductor', 'groin', 'inner thigh']],
	['abductors', ['abductor', 'outer thigh', 'outer hip']],
	['calves', ['calf', 'calves']],
	['neck', ['neck']]
];
const COARSE: [string, readonly MuscleGroup[]][] = [
	['back', COARSE_GROUPS.back],
	['shoulder', COARSE_GROUPS.shoulders],
	['delt', COARSE_GROUPS.shoulders],
	['leg', COARSE_GROUPS.legs],
	['arm', COARSE_GROUPS.arms]
];

/**
 * The muscle groups a body note's free-text location is about ("lower_back", "left hamstring", "shoulders"),
 * or none ("left knee"). A specific muscle wins over a region word in the same text.
 */
export function locationMuscles(location: string): MuscleGroup[] {
	const text = ` ${location.toLowerCase().replace(/[_-]/g, ' ')} `;
	const exact = MUSCLE_GROUPS.filter((m) => text.includes(m.replace('_', ' ')));
	const words = WORDS.filter(([, w]) => w.some((x) => text.includes(x))).map(([m]) => m);
	const specific = [...new Set([...exact, ...words])];
	// "inner thigh" also reads as quads; "lower back" also contains "back": the most specific match wins.
	if (specific.includes('adductors') || specific.includes('abductors'))
		return specific.filter((m) => m !== 'quads');
	if (specific.length) return specific;
	return [...new Set(COARSE.filter(([w]) => text.includes(w)).flatMap(([, ms]) => ms))];
}

/** A browser time zone name safe to put in SQL; anything odd falls back to UTC. */
export const sqlTimeZone = (tz = Intl.DateTimeFormat().resolvedOptions().timeZone) =>
	/^[A-Za-z0-9_+\-/]+$/.test(tz) ? tz : 'UTC';
