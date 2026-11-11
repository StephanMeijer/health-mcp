/** The vitals views the heart rate tab reads (src/tools.rs SCHEMA_DOC) and their value columns. */
export const SIGNALS = {
	heart_rate: { columns: ['bpm'], rawUpTo: 15 * 60, gap: 30 },
	ecg: { columns: ['uv'], rawUpTo: 7, gap: 0.5 }, // 130 Hz
	acc: { columns: ['x', 'y', 'z'], rawUpTo: 5, gap: 0.5 } // 200 Hz
} as const;
export type Signal = keyof typeof SIGNALS;

/** ECG and movement load only in windows this short: a whole day is millions of samples. */
export const DETAIL_UP_TO = 15 * 60;

// sql_query returns at most 1000 rows: stay under that with room to spare.
const MAX_POINTS = 900;
const STEPS = [
	0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600
];

/** Seconds per bucket for a window of `span` seconds: 0 means raw samples. A whole day gets 2 minutes. */
export function bucketFor(signal: Signal, span: number) {
	if (span <= SIGNALS[signal].rawUpTo) return 0;
	return STEPS.find((s) => span / s <= MAX_POINTS) ?? STEPS[STEPS.length - 1];
}

/**
 * The SQL for one signal between two epoch seconds: raw samples, or per-bucket averages (plus min and max for a
 * single column) with buckets starting at `from`. Only numbers go in, so nothing here needs escaping.
 */
export function windowSql(signal: Signal, from: number, to: number) {
	const { columns } = SIGNALS[signal];
	const step = bucketFor(signal, to - from);
	const where = `FROM ${signal} WHERE time >= to_timestamp(${from}) AND time < to_timestamp(${to})`;
	if (!step)
		return `SELECT extract(epoch FROM time)::float8 AS t, ${columns.join(', ')} ${where} ORDER BY time LIMIT 1000`;
	// The same columns as keysFor().
	const values = columns.map((c) => `avg(${c})::float8 AS ${c}`);
	if (columns.length === 1)
		values.push(
			`min(${columns[0]}) AS ${columns[0]}_min`,
			`max(${columns[0]}) AS ${columns[0]}_max`
		);
	return `SELECT extract(epoch FROM time_bucket('${step} seconds'::interval, time, to_timestamp(${from})))::float8 AS t, ${values.join(', ')} ${where} GROUP BY 1 ORDER BY 1`;
}

/** The columns a result holds: buckets of a single signal add its min and max for the band. */
export function keysFor(signal: Signal, step: number): string[] {
	const { columns } = SIGNALS[signal];
	return step && columns.length === 1
		? [columns[0], `${columns[0]}_min`, `${columns[0]}_max`]
		: [...columns];
}

/** A bucket width for people: 0.05 → "50 ms", 120 → "2 min". */
export const stepText = (step: number) =>
	step < 1 ? `${Math.round(step * 1000)} ms` : step < 60 ? `${step} s` : `${step / 60} min`;

export type Row = Record<string, number | null>;

/**
 * Rows → uPlot's columns `[t, ...keys]`. A gap longer than `gap` seconds (the strap was off) gets a null row,
 * so the line breaks there instead of bridging it.
 */
export function toColumns(rows: Row[], keys: string[], gap: number) {
	const cols: (number | null)[][] = [[], ...keys.map(() => [])];
	let prev = -Infinity;
	for (const r of rows) {
		const t = r.t as number;
		if (t - prev > gap && prev !== -Infinity) {
			cols[0].push((prev + t) / 2);
			for (let i = 1; i < cols.length; i++) cols[i].push(null);
		}
		cols[0].push(t);
		keys.forEach((k, i) => cols[i + 1].push(r[k] ?? null));
		prev = t;
	}
	return cols;
}

/** The gap that breaks a line: a missing bucket, or for raw samples the signal's own. */
export const gapFor = (signal: Signal, step: number) => (step ? step * 1.5 : SIGNALS[signal].gap);

/** Local midnight to the next local midnight of a `YYYY-MM-DD` day, in epoch seconds (23 or 25 hours on DST days). */
export function dayBounds(day: string): [number, number] {
	const [y, m, d] = day.split('-').map(Number);
	return [new Date(y, m - 1, d).getTime() / 1000, new Date(y, m - 1, d + 1).getTime() / 1000];
}
