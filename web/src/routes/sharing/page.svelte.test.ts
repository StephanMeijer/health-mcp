import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tool, ToolUserError } from '#lib/api/client.js';
import type { ShareList } from '#lib/api/results.js';
import { logbook } from '#lib/logbook.svelte.js';
import WithQuery from '#lib/test/with-query.svelte';
import Page from './+page.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => `/app${p}` }));

const list: ShareList = {
	your_email: 'me@example.com',
	shared_by_you: [
		{ email: 'bob@example.com', gym: 'ro', vitals: 'none', since: '2026-10-01T10:00:00Z' }
	],
	shared_with_you: [
		{ owner: 'cat@example.com', gym: 'ro', vitals: 'ro', since: '2026-10-02T10:00:00Z' }
	],
	invites_sent: [
		{ email: 'dan@example.com', gym: 'ro', vitals: 'none', expires_at: '2026-11-21T10:00:00Z' }
	],
	invites_received: [
		{
			invite_id: 7,
			from: 'ann@example.com',
			gym: 'rw',
			vitals: 'ro',
			expires_at: '2026-11-20T10:00:00Z'
		}
	]
};

const mocked = vi.mocked(tool);

beforeEach(() => {
	mocked.mockReset();
	mocked.mockImplementation(async (name) =>
		name === 'share_list'
			? list
			: name === 'share_respond'
				? { accepted: true, owner: 'ann@example.com' }
				: name === 'share_remove'
					? { email: 'x@example.com', removed: 'revoke' }
					: {}
	);
	// bits-ui's select calls these, which jsdom lacks.
	Element.prototype.scrollIntoView = vi.fn();
	Element.prototype.hasPointerCapture = vi.fn(() => false);
	Element.prototype.releasePointerCapture = vi.fn();
});

describe('sharing page', () => {
	it('changes one level with share_update', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, { component: Page });
		// bits-ui's select doesn't open from a jsdom click; the keyboard works (End: the last level, read & write).
		(await screen.findByRole('combobox', { name: 'Vitals for bob@example.com' })).focus();
		await user.keyboard('{Enter}');
		await screen.findAllByRole('option', { hidden: true });
		await user.keyboard('{End}{Enter}');
		await waitFor(() =>
			expect(mocked).toHaveBeenCalledWith(
				'share_update',
				{ email: 'bob@example.com', vitals: 'rw' },
				expect.anything()
			)
		);
	});

	it('shows the stored level again when share_update fails', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		const ok = mocked.getMockImplementation()!;
		mocked.mockImplementation(async (name, ...rest) => {
			if (name === 'share_update') throw new ToolUserError('nope');
			return ok(name, ...rest);
		});
		render(WithQuery, { component: Page });
		const level = await screen.findByRole('combobox', { name: 'Vitals for bob@example.com' });
		level.focus();
		await user.keyboard('{Enter}');
		await screen.findAllByRole('option', { hidden: true });
		await user.keyboard('{End}{Enter}');
		await waitFor(() =>
			expect(mocked).toHaveBeenCalledWith('share_update', expect.anything(), expect.anything())
		);
		await waitFor(() => expect(level.textContent).toContain('None'));
	});

	it('refreshes your account after accepting an invite', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		const refreshMe = vi.spyOn(logbook, 'refreshMe').mockResolvedValue();
		render(WithQuery, { component: Page });
		const invite = (await screen.findByText(/ann@example.com wants to share/)).closest(
			'div'
		)!.parentElement!;
		await user.click(within(invite).getByRole('button', { name: 'Accept' }));
		await waitFor(() => expect(refreshMe).toHaveBeenCalledOnce());
		expect(mocked).toHaveBeenCalledWith(
			'share_respond',
			{ invite_id: 7, accept: true },
			expect.anything()
		);
	});

	it.each([
		['Stop sharing', 'bob@example.com', 'revoke'],
		['Leave', 'cat@example.com', 'leave'],
		['Withdraw invite', 'dan@example.com', 'withdraw']
	])('%s on %s sends share_remove what=%s', async (label, email, what) => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		vi.spyOn(logbook, 'refreshMe').mockResolvedValue();
		render(WithQuery, { component: Page });
		await user.click(await screen.findByRole('button', { name: label }));
		const dialog = await screen.findByRole('alertdialog');
		await user.click(within(dialog).getByRole('button', { name: label }));
		await waitFor(() =>
			expect(mocked).toHaveBeenCalledWith('share_remove', { email, what }, expect.anything())
		);
	});
});
