import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { SleepSession } from '#lib/api/results.js';
import Page from './+page.svelte';

const night: SleepSession = {
	id: 'a',
	source: 'app',
	start_at: '2026-11-08T23:00:00Z',
	end_at: '2026-11-09T07:00:00Z',
	note: null,
	hours: 8,
	stage_seconds: null,
	stages: null
};
const result = vi.hoisted(() => ({ data: undefined as unknown }));

vi.mock('#lib/api/query.svelte.js', () => ({
	toolQuery: () => ({
		isPending: false,
		isError: false,
		get data() {
			return result.data;
		}
	}),
	toolMutation: () => ({})
}));

describe('Sleep page', () => {
	it('says when the period holds more nights than it lists', () => {
		result.data = { truncated: true, sessions: [night] };
		render(Page);
		expect(screen.getByText(/oldest 1000 nights/)).toBeInTheDocument();
	});

	it('says nothing when every night is listed', () => {
		result.data = [night];
		render(Page);
		expect(screen.queryByText(/oldest 1000 nights/)).toBeNull();
	});
});
