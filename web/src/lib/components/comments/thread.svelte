<script lang="ts">
	import ConfirmDelete from '#lib/components/confirm-delete.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { Thread } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';

	/**
	 * One comment thread: its comments oldest first, a reply box, and per comment edit (your own) and delete (your own,
	 * or any on your own logbook). Resolve or reopen when the server says you may (`can_resolve`).
	 */
	let { thread }: { thread: Thread } = $props();

	const opts = { invalidate: ['comment_list' as const] };
	const reply = toolMutation<Thread, 'comment_reply'>('comment_reply', {
		...opts,
		success: 'Reply added'
	});
	const update = toolMutation<Thread, 'comment_update'>('comment_update', {
		...opts,
		success: 'Comment saved'
	});
	const remove = toolMutation<Thread, 'comment_delete'>('comment_delete', {
		...opts,
		success: 'Comment deleted'
	});
	const resolve = toolMutation<Thread, 'thread_resolve'>('thread_resolve', {
		...opts,
		success: (t) => (t.resolved ? 'Thread resolved' : 'Thread reopened')
	});

	let text = $state('');
	let editing = $state<number | null>(null);
	let draft = $state('');
	const id = $derived(`thread-${thread.thread_id}`);
	let ownLogbook = $derived(logbook.owner === null);

	function submitReply(e: SubmitEvent) {
		e.preventDefault();
		reply.mutate({ thread_id: thread.thread_id, body: text }, { onSuccess: () => (text = '') });
	}

	function submitEdit(e: SubmitEvent) {
		e.preventDefault();
		if (editing === null) return;
		update.mutate({ comment_id: editing, body: draft }, { onSuccess: () => (editing = null) });
	}

	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<article class="grid gap-3 rounded-lg border p-3" aria-labelledby="{id}-title">
	<h3 id="{id}-title" class="sr-only">Thread from {when(thread.created_at)}</h3>
	{#if thread.resolved || thread.can_resolve}
		<header class="flex flex-wrap items-center gap-2">
			{#if thread.resolved}<Badge variant="outline">Resolved</Badge>{/if}
			{#if thread.can_resolve}
				<Button
					variant="ghost"
					size="sm"
					class="ml-auto"
					disabled={resolve.isPending}
					onclick={() =>
						resolve.mutate({ thread_id: thread.thread_id, resolved: !thread.resolved })}
					>{thread.resolved ? 'Reopen' : 'Resolve'}</Button
				>
			{/if}
		</header>
	{/if}
	<ol class="grid gap-3">
		{#each thread.comments as c (c.id)}
			<li class="grid gap-1">
				<div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
					<span class="font-medium break-all">{c.author}</span>
					{#if c.yours}<Badge variant="secondary">you</Badge>
					{:else if c.by_owner}<Badge variant="secondary">owner</Badge>{/if}
					<span class="text-xs text-muted-foreground tabular-nums">{when(c.created_at)}</span>
					{#if c.edited_at && !c.deleted}
						<span class="text-xs text-muted-foreground">(edited {when(c.edited_at)})</span>
					{/if}
				</div>
				{#if c.deleted}
					<p class="text-sm text-muted-foreground italic">Deleted</p>
				{:else if editing === c.id}
					<form class="grid gap-2" onsubmit={submitEdit}>
						<Label for="{id}-edit-{c.id}" class="sr-only">Edit comment</Label>
						<Textarea id="{id}-edit-{c.id}" rows={3} required bind:value={draft} />
						<div class="flex gap-2">
							<Button type="submit" size="sm" disabled={update.isPending}>Save comment</Button>
							<Button variant="ghost" size="sm" onclick={() => (editing = null)}>Cancel</Button>
						</div>
					</form>
				{:else}
					<p class="text-sm whitespace-pre-line">{c.body}</p>
					{#if c.yours || ownLogbook}
						<div class="-ml-2 flex gap-1">
							{#if c.yours}
								<Button
									variant="ghost"
									size="sm"
									onclick={() => {
										draft = c.body ?? '';
										editing = c.id;
									}}>Edit</Button
								>
							{/if}
							<ConfirmDelete
								label="Delete comment"
								description="The comment shows as deleted; replies to it stay."
								onconfirm={() => remove.mutate({ comment_id: c.id })}
							/>
						</div>
					{/if}
				{/if}
			</li>
		{/each}
	</ol>
	<form class="grid gap-2" onsubmit={submitReply}>
		<Label for="{id}-reply" class="sr-only">Reply</Label>
		<Textarea
			id="{id}-reply"
			rows={2}
			required
			placeholder={thread.resolved ? 'Reply (reopens the thread)' : 'Reply'}
			bind:value={text}
		/>
		<div><Button type="submit" size="sm" disabled={reply.isPending}>Reply</Button></div>
	</form>
</article>
