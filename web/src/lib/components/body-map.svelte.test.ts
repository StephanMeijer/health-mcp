import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import BodyMap from './body-map.svelte';

describe('BodyMap', () => {
	it('names each muscle with its sets and soreness, and outlines the sore ones', () => {
		const { container } = render(BodyMap, {
			sets: { chest: 12, quads: 3.5 },
			sore: { quads: 6, neck: null }
		});
		const titles = [...container.querySelectorAll('title')].map((t) => t.textContent);
		expect(titles).toContain('Chest: 12 sets');
		expect(titles).toContain('Quads: 3.5 sets, sore (6/10)');
		expect(titles).toContain('Neck: 0 sets, sore');
		const quads = [...container.querySelectorAll('g')].find((g) =>
			g.querySelector('title')?.textContent?.startsWith('Quads')
		);
		expect(quads?.getAttribute('class')).toContain('stroke-destructive');
		// The table fallback lists every muscle.
		expect(screen.getByRole('cell', { name: 'Yes, 6/10' })).toBeTruthy();
		expect(screen.getAllByRole('row')).toHaveLength(19);
	});
});
