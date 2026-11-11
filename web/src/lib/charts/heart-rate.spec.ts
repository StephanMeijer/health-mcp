import { describe, expect, it } from 'vitest';
import { bucketFor, keysFor, stepText, toColumns, windowSql } from './heart-rate.js';
import { uplotTheme } from './theme.js';

describe('bucketFor', () => {
	it('shows a whole day of heart rate in 2-minute buckets', () => {
		expect(bucketFor('heart_rate', 86_400)).toBe(120);
		// A 25-hour DST day still fits under the 1000-row cap.
		expect(bucketFor('heart_rate', 90_000)).toBe(120);
	});

	it('switches to raw samples once the window is short enough for each signal', () => {
		expect(bucketFor('heart_rate', 15 * 60)).toBe(0);
		expect(bucketFor('heart_rate', 15 * 60 + 1)).toBe(2);
		expect(bucketFor('ecg', 7)).toBe(0);
		expect(bucketFor('ecg', 8)).toBe(0.01);
		expect(bucketFor('acc', 5)).toBe(0);
		expect(bucketFor('acc', 6)).toBe(0.01);
	});

	it('keeps every window under 1000 rows', () => {
		for (const span of [8, 30, 61, 600, 900, 3600, 7200, 86_400]) {
			const step = bucketFor('ecg', span);
			expect(span / step).toBeLessThan(1000);
		}
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
		expect(windowSql('acc', 10, 14)).toBe(
			'SELECT extract(epoch FROM time)::float8 AS t, x, y, z FROM acc WHERE time >= to_timestamp(10) AND time < to_timestamp(14) ORDER BY time LIMIT 1000'
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
