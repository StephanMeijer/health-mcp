import { afterEach, describe, expect, it } from 'vitest';
import { bucketFor, dayBounds, keysFor, stepText, toColumns, windowSql } from './heart-rate.js';
import { uplotTheme } from './theme.js';

describe('bucketFor', () => {
	it('shows a whole day of heart rate in 2-minute buckets', () => {
		expect(bucketFor('heart_rate', 86_400)).toBe(120);
		// A 25-hour DST day still fits under the 1000-row cap.
		expect(bucketFor('heart_rate', 90_000)).toBe(120);
	});

	it('switches to raw samples once the window is short enough for ECG and movement', () => {
		expect(bucketFor('ecg', 7)).toBe(0);
		expect(bucketFor('ecg', 8)).toBe(0.01);
		// 4 s × 200 Hz = 800 rows: room under the cap.
		expect(bucketFor('acc', 4)).toBe(0);
		expect(bucketFor('acc', 5)).toBe(0.01);
	});

	it('averages heart rate per second at the finest, so overlapping sources fit', () => {
		expect(bucketFor('heart_rate', 15 * 60)).toBe(1);
		expect(bucketFor('heart_rate', 15 * 60 + 1)).toBe(2);
		expect(bucketFor('heart_rate', 2)).toBe(1);
	});

	it('keeps every window under 1000 rows', () => {
		for (const span of [8, 30, 61, 600, 900, 3600, 7200, 86_400]) {
			const step = bucketFor('ecg', span);
			expect(span / step).toBeLessThan(1000);
		}
		for (const span of [2, 600, 900, 3600, 90_000])
			expect(span / bucketFor('heart_rate', span)).toBeLessThan(1000);
	});
});

describe('windowSql', () => {
	it('buckets from the window start with average, min and max', () => {
		const sql = windowSql('heart_rate', 1000, 87_400);
		expect(sql).toContain("time_bucket('120 seconds'::interval, time, to_timestamp(1000))");
		expect(sql).toContain('min(bpm) AS bpm_min');
		expect(sql).toContain('time >= to_timestamp(1000) AND time < to_timestamp(87400)');
	});

	it('reads raw samples of every axis in a short window', () => {
		// No LIMIT of its own: sql_query's cap then reports `truncated`.
		expect(windowSql('acc', 10, 14)).toBe(
			'SELECT extract(epoch FROM time)::float8 AS t, x, y, z FROM acc WHERE time >= to_timestamp(10) AND time < to_timestamp(14) ORDER BY time'
		);
	});
});

describe('toColumns', () => {
	it('turns rows into columns in key order', () => {
		const rows = [
			{ t: 0, bpm: 60, bpm_min: 55, bpm_max: 70 },
			{ t: 120, bpm: 62, bpm_min: null, bpm_max: 71 }
		];
		expect(toColumns(rows, keysFor('heart_rate', 120), 180)).toEqual([
			[0, 120],
			[60, 62],
			[55, null],
			[70, 71]
		]);
	});

	it('breaks the line where the strap was off', () => {
		expect(
			toColumns(
				[
					{ t: 0, uv: 1 },
					{ t: 1, uv: 2 },
					{ t: 10, uv: 3 }
				],
				['uv'],
				2
			)
		).toEqual([
			[0, 1, 5.5, 10],
			[1, 2, null, 3]
		]);
	});
});

it('names bucket widths for people', () => {
	expect([0.05, 2, 120].map(stepText)).toEqual(['50 ms', '2 s', '2 min']);
});

it('reads uPlot colours from the theme tokens', () => {
	const theme = uplotTheme((n) => (n === '--chart-1' ? 'hsl(1 2% 3%)' : '#000'));
	expect(theme.colors[0]).toBe('hsl(1, 2%, 3%)');
	expect(theme.axis.grid.stroke).toBe('#000');
});

describe('dayBounds', () => {
	const tz = process.env.TZ;
	afterEach(() => {
		process.env.TZ = tz;
	});

	it('spans 23 and 25 hours on DST days in the browser’s time zone', () => {
		process.env.TZ = 'Europe/Amsterdam';
		const span = (day: string) => dayBounds(day)[1] - dayBounds(day)[0];
		expect(span('2026-03-29')).toBe(82_800);
		expect(span('2026-10-25')).toBe(90_000);
		expect(span('2026-11-11')).toBe(86_400);
		expect(dayBounds('2026-11-11')[0]).toBe(Date.UTC(2026, 10, 10, 23) / 1000);
	});
});
