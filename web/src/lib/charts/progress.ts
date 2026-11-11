import type { Progress } from '#lib/api/results.js';

/** What a machine/variant setup is called in a legend or table. */
export const setupName = (s: { machine: string | null; variant: string | null }) =>
	[s.machine, s.variant].filter(Boolean).join(' · ') || 'No machine noted';

/** progress rows → one line per machine/variant (weights don't compare across machines), points in date order. */
export function progressLines(rows: Progress['series']) {
	const lines = new Map<string, { name: string; points: [string, number][] }>();
	for (const r of rows) {
		// The server's SETUP_KEY (src/gym/search.rs): case-insensitive, machine and variant kept apart.
		const key = `${r.machine?.toLowerCase() ?? ''}|${r.variant?.toLowerCase() ?? ''}`;
		if (!lines.has(key)) lines.set(key, { name: setupName(r), points: [] });
		lines.get(key)!.points.push([r.period_start, r.value]);
	}
	return [...lines.values()].map(({ name, points }) => ({
		name,
		points: points.sort((a, b) => a[0].localeCompare(b[0]))
	}));
}
