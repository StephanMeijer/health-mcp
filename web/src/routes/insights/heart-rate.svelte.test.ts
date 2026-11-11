import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import HeartRate from './heart-rate.svelte';

const vitals = vi.hoisted(() => ({ readable: false }));
vi.mock('#lib/logbook.svelte.js', () => ({
	logbook: { owner: 'jamie@trainer.nl', canRead: () => vitals.readable }
}));
// uPlot needs a canvas and matchMedia, which jsdom lacks: the chart itself is checked in a browser.
vi.mock('#lib/charts/uplot.svelte', () => ({ default: () => {} }));
vi.mock('@tanstack/svelte-query', () => ({
	createQuery: () => ({
		isPending: false,
		isError: false,
		isPlaceholderData: false,
		data: { step: 120, truncated: true, rows: [{ t: 0, bpm: 60, bpm_min: 55, bpm_max: 70 }] }
	})
}));

describe('Heart rate tab', () => {
	it('says so when the logbook does not share vitals', () => {
		vitals.readable = false;
		render(HeartRate);
		expect(screen.getByText(/doesn’t share with you/)).toBeInTheDocument();
		expect(screen.queryByLabelText('Day')).toBeNull();
	});

	it('says when a window holds more readings than one query returns', () => {
		vitals.readable = true;
		render(HeartRate);
		expect(screen.getByLabelText('Day')).toBeInTheDocument();
		expect(screen.getAllByText(/Too many readings in this window/)).toHaveLength(1);
	});
});
