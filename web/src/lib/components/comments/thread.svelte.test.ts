import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tool } from '#lib/api/client.js';
import type { Comment, Thread } from '#lib/api/results.js';
import { logbook } from '#lib/logbook.svelte.js';
import WithQuery from '#lib/test/with-query.svelte';
import ThreadView from './thread.svelte';
import ThreadList from './thread-list.svelte';

vi.mock('#lib/api/client.js', async (orig) => ({
	...(await orig<typeof import('#lib/api/client.js')>()),
	tool: vi.fn()
}));

const mocked = vi.mocked(tool);

const comment = (c: Partial<Comment>): Comment => ({
	id: 1,
	author: 'jamie@trainer.nl',
	by_owner: false,
	yours: false,
	body: 'Knees out on the way up.',
	deleted: false,
	created_at: '2026-11-10T19:00:00Z',
	edited_at: null,
	...c
});

const thread = (t: Partial<Thread> = {}): Thread => ({
	thread_id: 5,
	target: 'set',
	workout_id: null,
	exercise_id: null,
	set_id: 11,
	exercise: 'Leg press',
	resolved: false,
	resolved_at: null,
	created_at: '2026-11-10T19:00:00Z',
	can_resolve: false,
	comments: [comment({})],
	...t
});

function setup(t: Thread) {
	const user = userEvent.setup({ pointerEventsCheck: 0 });
	render(WithQuery, { props: { component: ThreadView, props: { thread: t } } });
	return user;
}

beforeEach(() => {
	mocked.mockReset();
	mocked.mockImplementation(async () => thread());
});
afterEach(() => (logbook.owner = null));

describe('Thread', () => {
	it('lets you edit only your own comments', async () => {
		logbook.owner = 'jamie@trainer.nl';
		const user = setup(
			thread({
				comments: [
					comment({ id: 1, by_owner: true, author: 'steve@example.com' }),
					comment({ id: 2, yours: true, body: 'Will do' })
				]
			})
		);
		expect(screen.getAllByRole('button', { name: 'Edit' })).toHaveLength(1);
		expect(screen.getByText('owner')).toBeInTheDocument();
		expect(screen.getByText('you')).toBeInTheDocument();
		await user.click(screen.getByRole('button', { name: 'Edit' }));
		const box = screen.getByLabelText('Edit comment');
		await user.clear(box);
		await user.type(box, 'Done');
		await user.click(screen.getByRole('button', { name: 'Save comment' }));
		expect(mocked).toHaveBeenCalledWith(
			'comment_update',
			{ comment_id: 2, body: 'Done' },
			{ owner: 'jamie@trainer.nl' }
		);
	});

	it('offers delete on your own comments in someone else’s logbook', () => {
		logbook.owner = 'jamie@trainer.nl';
		setup(thread({ comments: [comment({ id: 1 }), comment({ id: 2, yours: true })] }));
		expect(screen.getAllByRole('button', { name: 'Delete comment' })).toHaveLength(1);
	});

	it('offers delete on every comment in your own logbook', () => {
		setup(thread({ comments: [comment({ id: 1 }), comment({ id: 2 })] }));
		expect(screen.getAllByRole('button', { name: 'Delete comment' })).toHaveLength(2);
		expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
	});

	it('resolves only with can_resolve', async () => {
		const { unmount } = render(WithQuery, {
			props: { component: ThreadView, props: { thread: thread() } }
		});
		expect(screen.queryByRole('button', { name: 'Resolve' })).toBeNull();
		unmount();
		const user = setup(thread({ can_resolve: true, resolved: true }));
		await user.click(screen.getByRole('button', { name: 'Reopen' }));
		expect(mocked).toHaveBeenCalledWith(
			'thread_resolve',
			{ thread_id: 5, resolved: false },
			{ owner: null }
		);
	});

	it('shows a deleted comment in place, without actions', () => {
		setup(thread({ comments: [comment({ deleted: true, body: null, yours: true })] }));
		expect(screen.getByText('Deleted')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Delete comment' })).toBeNull();
	});

	it('replies to the thread', async () => {
		const user = setup(thread());
		await user.type(screen.getByLabelText('Reply'), 'Thanks!');
		await user.click(screen.getByRole('button', { name: 'Reply' }));
		expect(mocked).toHaveBeenCalledWith(
			'comment_reply',
			{ thread_id: 5, body: 'Thanks!' },
			{ owner: null }
		);
	});
});

describe('ThreadList', () => {
	it('keeps resolved threads behind "Show resolved" and starts threads on its target', async () => {
		const user = userEvent.setup({ pointerEventsCheck: 0 });
		render(WithQuery, {
			props: {
				component: ThreadList,
				props: {
					target: 'set',
					id: 11,
					threads: [thread({ resolved: true, comments: [comment({ body: 'Old news' })] })]
				}
			}
		});
		expect(screen.queryByText('Old news')).toBeNull();
		await user.click(screen.getByRole('button', { name: 'Show resolved (1)' }));
		expect(screen.getByText('Old news')).toBeInTheDocument();
		await user.type(screen.getByLabelText('Start a thread'), 'Felt easy?');
		await user.click(screen.getByRole('button', { name: 'Start thread' }));
		expect(mocked).toHaveBeenCalledWith(
			'thread_start',
			{ target: 'set', id: 11, body: 'Felt easy?' },
			{ owner: null }
		);
	});
});
