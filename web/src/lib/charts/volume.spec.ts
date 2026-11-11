import { describe, expect, it } from 'vitest';
import {
	isoDay,
	locationMuscles,
	muscleShade,
	regionSeries,
	sqlTimeZone,
	weekStarts
} from './volume.js';

describe('muscleShade', () => {
	it('steps from none through the 10-20 sets a week target to above it', () => {
		expect([0, 0.5, 4.5, 5, 9.5, 10, 20, 20.5].map(muscleShade)).toEqual([0, 1, 1, 2, 2, 3, 3, 4]);
	});
});

describe('locationMuscles', () => {
	it('matches stored names, everyday words and regions', () => {
		expect(locationMuscles('lower_back')).toEqual(['lower_back']);
		expect(locationMuscles('Left hamstring')).toEqual(['hamstrings']);
		expect(locationMuscles('right calf')).toEqual(['calves']);
		expect(locationMuscles('traps')).toEqual(['upper_back']);
		expect(locationMuscles('shoulders')).toEqual(['front_delts', 'side_delts', 'rear_delts']);
	});

	it('prefers the specific muscle over a region word in the same text', () => {
		expect(locationMuscles('lower back')).toEqual(['lower_back']);
		expect(locationMuscles('back of shoulder')).toEqual(['rear_delts']);
		expect(locationMuscles('inner thigh')).toEqual(['adductors']);
		expect(locationMuscles('forearm')).toEqual(['forearms']);
	});

	it('matches nothing for places that are not a muscle', () => {
		expect(locationMuscles('left knee')).toEqual([]);
		expect(locationMuscles('elbow')).toEqual([]);
	});
});

describe('weekStarts', () => {
	it('gives the Mondays of the last weeks, this week last', () => {
		// Wednesday 11 November 2026.
		expect(weekStarts(3, new Date(2026, 10, 11, 15)).map(isoDay)).toEqual([
			'2026-10-26',
			'2026-11-02',
			'2026-11-09'
		]);
		// A Sunday belongs to the week that started the Monday before.
		expect(weekStarts(1, new Date(2026, 10, 15)).map(isoDay)).toEqual(['2026-11-09']);
	});
});

describe('regionSeries', () => {
	it('sums muscles into regions per week and leaves out regions without sets', () => {
		expect(
			regionSeries([
				[
					{ muscle_group: 'chest', sets: 3 },
					{ muscle_group: 'lats', sets: 2 },
					{ muscle_group: 'upper_back', sets: 1.5 }
				],
				[{ muscle_group: 'chest', sets: 4 }]
			])
		).toEqual([
			{ name: 'Chest', data: [3, 4] },
			{ name: 'Back', data: [3.5, 0] }
		]);
	});
});

describe('sqlTimeZone', () => {
	it('passes zone names and refuses anything that could break out of a SQL string', () => {
		expect(sqlTimeZone('Europe/Amsterdam')).toBe('Europe/Amsterdam');
		expect(sqlTimeZone("x'; DROP")).toBe('UTC');
	});
});
