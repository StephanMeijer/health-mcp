<script lang="ts">
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { toolMutation } from '#lib/api/query.svelte.js';
	import type { Thread } from '#lib/api/results.js';
	import type { Target } from '#lib/api/tools.gen.js';
	import ThreadView from './thread.svelte';

	/**
	 * The threads about one workout, exercise or set, open ones first and resolved ones behind "Show resolved", with a
	 * box to start a new one. `threads` undefined is still loading; `error` a failed load.
	 */
	let {
		target,
		id,
		threads,
		error = null
	}: { target: Target; id: number; threads: Thread[] | undefined; error?: Error | null } = $props();

	const start = toolMutation<Thread, 'thread_start'>('thread_start', {
		invalidate: ['comment_list'],
		success: 'Thread started'
	});

	let text = $state('');
	let showResolved = $state(false);
	let open = $derived((threads ?? []).filter((t) => !t.resolved));
	let resolved = $derived((threads ?? []).filter((t) => t.resolved));
	const field = $derived(`start-${target}-${id}`);

	function submit(e: SubmitEvent) {
		e.preventDefault();
		start.mutate({ target, id, body: text }, { onSuccess: () => (text = '') });
	}
</script>

<div class="grid gap-3">
	{#if error}
		<p class="text-sm text-destructive">Comments could not be loaded: {error.message}</p>
	{:else if threads === undefined}
		<Skeleton class="h-20 w-full" />
	{:else}
		{#if !open.length}
			<p class="text-sm text-muted-foreground">
				{resolved.length
					? 'No open threads.'
					: 'No comments yet. Start a thread to ask something or give feedback; everyone who can see this logbook can reply.'}
			</p>
		{/if}
		{#each open as t (t.thread_id)}<ThreadView thread={t} />{/each}
		{#if resolved.length}
			<div>
				<Button
					variant="link"
					class="px-0"
					aria-expanded={showResolved}
					onclick={() => (showResolved = !showResolved)}
					>{showResolved ? 'Hide resolved' : `Show resolved (${resolved.length})`}</Button
				>
			</div>
			{#if showResolved}
				{#each resolved as t (t.thread_id)}<ThreadView thread={t} />{/each}
			{/if}
		{/if}
	{/if}
	<form class="grid gap-2" onsubmit={submit}>
		<Label for={field}>Start a thread</Label>
		<Textarea id={field} rows={2} required bind:value={text} />
		<div><Button type="submit" size="sm" disabled={start.isPending}>Start thread</Button></div>
	</form>
</div>
