import type { KindInfo } from '#lib/kinds.js';
import type { ReadingRow, Source, Stage } from '#lib/api/results.js';
import type { Kind, Reading } from '#lib/api/tools.gen.js';

/** One reading_add call. */
export type AddCall = { kind: Kind; readings: Reading[] };

export const SOURCE_LABEL: Record<Source, string> = {
	manual: 'Logged by hand',
	app: 'From an app',
	import: 'Imported'
};

export const STAGES: { stage: Stage; label: string; color: string }[] = [
	{ stage: 'deep', label: 'Deep', color: 'bg-chart-3' },
	{ stage: 'light', label: 'Light', color: 'bg-chart-1' },
	{ stage: 'rem', label: 'REM', color: 'bg-chart-4' },
	{ stage: 'awake', label: 'Awake', color: 'bg-chart-2' }
];

/** Systolic and diastolic are logged together, with an optional pulse, as one blood pressure measurement. */
export const isBloodPressure = (kind: string) => kind === 'systolic' || kind === 'diastolic';

export const readingValue = (row: ReadingRow, kind: KindInfo) =>
	(row as unknown as Record<string, number>)[kind.field];

export const num = (n: number | null | undefined, digits = 1) =>
	n == null ? '–' : n.toLocaleString(undefined, { maximumFractionDigits: digits });

export const dateTime = (iso: string) =>
	new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });

/** RFC 3339 → the `YYYY-MM-DDTHH:mm` a datetime-local input shows, in the browser's timezone. */
export function toLocalInput(iso: string) {
	const d = new Date(iso);
	return new Date(d.getTime() - d.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}

/**
 * Runs the reading_add calls in order. When one fails, deletes the readings the earlier calls added (a blood pressure
 * measurement is all or nothing, and a retry would otherwise hit the unique time), then rethrows.
 */
export async function addAll(
	calls: AddCall[],
	add: (c: AddCall) => Promise<unknown>,
	remove: (args: { kind: Kind; times: string[] }) => Promise<unknown>
) {
	const done: AddCall[] = [];
	try {
		for (const c of calls) {
			await add(c);
			done.push(c);
		}
	} catch (e) {
		await Promise.allSettled(
			done.map((c) => remove({ kind: c.kind, times: c.readings.map((r) => r.time) }))
		);
		throw e;
	}
}

/** A datetime-local input's value (browser timezone) → RFC 3339 for the tools. */
export const fromLocalInput = (local: string) => new Date(local).toISOString();

/** The `from` of a range ending now. */
export const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString();

export const RANGES = [
	{ value: '7', label: 'Last 7 days' },
	{ value: '30', label: 'Last 30 days' },
	{ value: '90', label: 'Last 90 days' },
	{ value: '365', label: 'Last year' }
];
