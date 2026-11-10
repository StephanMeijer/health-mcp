import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tool } from '#lib/api/client.js';
import WithQuery from '#lib/test/with-query.svelte';
import ShareInvite from './share-invite.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));

beforeEach(() => {
	vi.mocked(tool).mockReset();
	// bits-ui's select calls these, which jsdom lacks.
	Element.prototype.scrollIntoView = vi.fn();
	Element.prototype.hasPointerCapture = vi.fn(() => false);
	Element.prototype.releasePointerCapture = vi.fn();
});

describe('ShareInvite', () => {
	it('refuses an invite that shares nothing', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, { component: ShareInvite });
		await user.click(screen.getByRole('button', { name: 'Invite someone' }));
		await user.type(await screen.findByLabelText('Email'), 'sam@example.com');
		await user.click(screen.getByRole('button', { name: 'Send invite' }));
		expect((await screen.findByRole('alert')).textContent).toContain('Choose read or read & write');
		expect(tool).not.toHaveBeenCalled();
	});

	it('shows an empty form again after Done', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		vi.mocked(tool).mockResolvedValue({
			invited: 'sam@example.com',
			gym: 'ro',
			vitals: 'none',
			expires_at: '2026-11-20T10:00:00Z',
			mailed: true,
			link: 'https://example.com/i/x'
		});
		render(WithQuery, { component: ShareInvite });
		await user.click(screen.getByRole('button', { name: 'Invite someone' }));
		await user.type(await screen.findByLabelText('Email'), 'sam@example.com');
		// bits-ui's select doesn't open from a jsdom click; the keyboard works (Down: read).
		screen.getByRole('combobox', { name: 'Gym logbook' }).focus();
		await user.keyboard('{Enter}');
		await screen.findAllByRole('option', { hidden: true });
		await user.keyboard('{Home}{ArrowDown}{Enter}');
		await user.click(screen.getByRole('button', { name: 'Send invite' }));
		await user.click(await screen.findByRole('button', { name: 'Done' }));
		await user.click(screen.getByRole('button', { name: 'Invite someone' }));
		expect(((await screen.findByLabelText('Email')) as HTMLInputElement).value).toBe('');
		expect(screen.getByRole('combobox', { name: 'Gym logbook' }).textContent).toContain('None');
		expect(screen.queryByText(/Mailed to/)).toBeNull();
	});
});
