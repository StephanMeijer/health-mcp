import { describe, expect, it, vi } from 'vitest';

const { tool, ToolUserError, ToolRetryError, ToolServerError, retryable, errorJson } =
	await import('./client.js');

function reply(status: number, body: unknown) {
	return vi.fn<typeof fetch>(async () => new Response(JSON.stringify(body), { status }));
}

describe('tool', () => {
	it('posts the arguments with the same-origin markers', async () => {
		const f = reply(200, { ok: true });
		await expect(tool('gym_list', {}, { fetch: f })).resolves.toEqual({ ok: true });
		const [url, init] = f.mock.calls[0];
		expect(url).toBe('/api/v1/tools/gym_list');
		expect(init?.credentials).toBe('same-origin');
		expect(new Headers(init?.headers).get('x-requested-with')).toBe('health-web');
	});

	it('adds owner only to tools that take it', async () => {
		const f = reply(200, {});
		await tool('gym_list', {}, { fetch: f, owner: 'ann@example.com' });
		await tool('share_list', {}, { fetch: f, owner: 'ann@example.com' });
		const sent = f.mock.calls.map(([, init]) => JSON.parse(String(init?.body)));
		expect(sent).toEqual([{ owner: 'ann@example.com' }, {}]);
	});

	it('maps statuses to errors', async () => {
		await expect(
			tool('gym_list', {}, { fetch: reply(400, { error: 'no gym with id 9' }) })
		).rejects.toThrow(new ToolUserError('no gym with id 9'));
		await expect(
			tool('gym_list', {}, { fetch: reply(503, { error: 'retry' }) })
		).rejects.toBeInstanceOf(ToolRetryError);
		await expect(tool('gym_list', {}, { fetch: reply(500, null) })).rejects.toThrow(
			new ToolServerError('the server failed (500)')
		);
	});

	it("keeps the server's message for other refusals", async () => {
		await expect(
			tool('gym_list', {}, { fetch: reply(403, { error: 'cross-site request refused' }) })
		).rejects.toThrow(new ToolServerError('cross-site request refused'));
	});

	it('sends a logged-out person to the login and never settles', async () => {
		const onLoggedOut = vi.fn();
		const settled = vi.fn();
		void tool('gym_list', {}, { fetch: reply(401, null), onLoggedOut }).then(settled, settled);
		await vi.waitFor(() => expect(onLoggedOut).toHaveBeenCalledOnce());
		await new Promise((r) => setTimeout(r, 10));
		expect(settled).not.toHaveBeenCalled();
	});

	it('retries only transient failures, three times', () => {
		expect(retryable(0, new ToolRetryError('x'))).toBe(true);
		expect(retryable(3, new ToolRetryError('x'))).toBe(false);
		expect(retryable(0, new ToolUserError('x'))).toBe(false);
	});
});

describe('errorJson', () => {
	it('reads the JSON inside a refusal', () => {
		const e = new ToolUserError(
			JSON.stringify({
				error: 'a workout is still open at Gym A (3 sets)',
				open_workout: { workout_id: 4 }
			})
		);
		expect(errorJson(e)).toEqual({
			error: 'a workout is still open at Gym A (3 sets)',
			open_workout: { workout_id: 4 }
		});
	});

	it('is null for a plain message or another error', () => {
		expect(errorJson(new ToolUserError('no gym with id 9'))).toBeNull();
		expect(errorJson(new ToolUserError('{not json'))).toBeNull();
		expect(errorJson(new ToolServerError('{}'))).toBeNull();
	});
});
