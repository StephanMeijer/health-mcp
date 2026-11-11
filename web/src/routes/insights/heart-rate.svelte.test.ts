import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import HeartRate from './heart-rate.svelte';

const vitals = vi.hoisted(() => ({ readable: false }));
// uPlot reads matchMedia on import, which jsdom lacks; no query runs on a logbook without vitals.
vi.mock('uplot', () => ({ default: class {} }));
vi.mock('@tanstack/svelte-query', () => ({ createQuery: () => ({}) }));
vi.mock('#lib/logbook.svelte.js', () => ({
	logbook: { owner: 'jamie@trainer.nl', canRead: () => vitals.readable }
}));

describe('Heart rate tab', () => {
	it('says so when the logbook does not share vitals', () => {
		render(HeartRate);
		expect(screen.getByText(/doesn’t share with you/)).toBeInTheDocument();
		expect(screen.queryByLabelText('Day')).toBeNull();
	});
});
