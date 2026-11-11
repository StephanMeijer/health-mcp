import type { Progress } from '#lib/api/results.js';

/** What a machine/variant setup is called in a legend or table. */
export const setupName = (s: { machine: string | null; variant: string | null }) =>
	[s.machine, s.variant].filter(Boolean).join(' · ') || 'No machine noted';

/** progress rows → one line per machine/variant (weights don't compare across machines), points in date order. */
export function progressLines(rows: Progress['series']) {
	const lines = new Map<string, [string, number][]>();
	for (const r of rows) {
		const name = setupName(r);
		if (!lines.has(name)) lines.set(name, []);
		lines.get(name)!.push([r.period_start, r.value]);
	}
	return [...lines].map(([name, points]) => ({
		name,
		points: points.sort((a, b) => a[0].localeCompare(b[0]))
	}));
}
