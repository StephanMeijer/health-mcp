import { QueryClient } from '@tanstack/svelte-query';
import { render } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { logbook } from '#lib/logbook.svelte.js';
import { tool } from './client.js';
import Probe from './mutation-probe.test.svelte';
import type { toolMutation } from './query.svelte.js';

const client = new QueryClient();
vi.mock('svelte-sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock('./client.js', async (orig) => ({
	...(await orig<typeof import('./client.js')>()),
	tool: vi.fn()
}));

function mutation() {
	let m!: ReturnType<typeof toolMutation<unknown, 'gym_create'>>;
	render(Probe, { client, ready: (x) => (m = x) });
	return m;
}

afterEach(() => {
	logbook.owner = null;
	vi.clearAllMocks();
});

describe('toolMutation', () => {
	it('refetches the logbook it wrote to, even after a switch', async () => {
		let finish!: (v: unknown) => void;
		vi.mocked(tool).mockReturnValue(new Promise((r) => (finish = r)));
		const invalidate = vi.spyOn(client, 'invalidateQueries');
		logbook.owner = 'ann@example.com';
		const m = mutation();
		const done = m.mutateAsync({ name: 'Gym' });
		await vi.waitFor(() => expect(tool).toHaveBeenCalled());
		logbook.owner = null;
		finish({});
		await done;
		expect(invalidate).toHaveBeenCalledWith({ queryKey: ['ann@example.com', 'gym_list'] });
	});
});
