import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import ConfirmDelete from './confirm-delete.svelte';

describe('ConfirmDelete', () => {
	it('deletes only after confirming', async () => {
		// jsdom never finishes the dialog's close animation, so bits-ui's pointer-events lock on body stays; skip that check.
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		const onconfirm = vi.fn();
		render(ConfirmDelete, {
			label: 'Delete reading',
			description: 'It is gone for good.',
			onconfirm
		});
		const trigger = screen.getByRole('button', { name: 'Delete reading' });
		await user.click(trigger);
		expect(onconfirm).not.toHaveBeenCalled();
		await user.click(await screen.findByRole('button', { name: 'Cancel' }));
		expect(onconfirm).not.toHaveBeenCalled();
		await user.click(trigger);
		const dialog = await screen.findByRole('alertdialog');
		await user.click(
			[...dialog.querySelectorAll('button')].find(
				(b) => b.textContent?.trim() === 'Delete reading'
			)!
		);
		expect(onconfirm).toHaveBeenCalledOnce();
	});
});
