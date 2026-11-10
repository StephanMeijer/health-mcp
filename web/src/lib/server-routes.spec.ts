import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { toastError } = vi.hoisted(() => ({ toastError: vi.fn() }));
vi.mock('svelte-sonner', () => ({ toast: { error: toastError } }));

const { logout } = await import('./server-routes.js');

describe('logout', () => {
	const assign = vi.fn();
	beforeEach(() => vi.stubGlobal('location', { assign }));
	afterEach(() => {
		vi.unstubAllGlobals();
		vi.clearAllMocks();
	});

	const answer = (status: number) =>
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => new Response(null, { status }))
		);

	it.each([204, 401])('goes to the login on %i', async (status) => {
		answer(status);
		await logout();
		expect(assign).toHaveBeenCalledWith('/app/login');
		expect(toastError).not.toHaveBeenCalled();
	});

	it('stays and says so when the server fails', async () => {
		answer(500);
		await logout();
		expect(assign).not.toHaveBeenCalled();
		expect(toastError).toHaveBeenCalledOnce();
	});

	it('stays and says so when the network fails', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => Promise.reject(new TypeError('offline')))
		);
		await expect(logout()).resolves.toBeUndefined();
		expect(assign).not.toHaveBeenCalled();
		expect(toastError).toHaveBeenCalledOnce();
	});
});
