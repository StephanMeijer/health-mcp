import { describe, expect, it } from 'vitest';
import { plateClass, plateFor } from './plates';

describe('plateFor', () => {
	it('maps loads to plate bands', () => {
		expect(plateFor(20)).toBe(5);
		expect(plateFor(40)).toBe(10);
		expect(plateFor(70)).toBe(15);
		expect(plateFor(100)).toBe(20);
		expect(plateFor(119.5)).toBe(20);
		expect(plateFor(120)).toBe(25);
		expect(plateFor(300)).toBe(25);
	});

	it('has no plate without a load', () => {
		for (const kg of [null, undefined, 0, -5, Number.NaN]) expect(plateFor(kg)).toBeNull();
	});

	it('names the theme class', () => {
		expect(plateClass(20)).toBe('bg-plate-20');
		expect(plateClass(null)).toBe('bg-transparent');
	});
});
