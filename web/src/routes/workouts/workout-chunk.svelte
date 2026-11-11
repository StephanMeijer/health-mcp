<script lang="ts">
	import EmptyState from '#lib/components/empty-state.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { Workout } from '#lib/api/results.js';
	import type { WorkoutListArgs } from '#lib/api/tools.gen.js';
	import { resolve } from '$app/paths';
	import WorkoutChunk from './workout-chunk.svelte';

	/**
	 * One page of the workout list: up to 20 workouts started before `to`, newest first. "Load older" renders the next
	 * page below, starting before the last one shown.
	 */
	let {
		filters,
		to,
		first = false,
		empty
	}: {
		filters: Omit<WorkoutListArgs, 'owner' | 'to' | 'limit'>;
		to: string | null;
		first?: boolean;
		empty: { title: string; hint: string };
	} = $props();

	const LIMIT = 20;
	const list = toolQuery<Workout[], 'workout_list'>('workout_list', () => ({
		...filters,
		to,
		limit: LIMIT
	}));
	// shortcut: `to` is exclusive, so a workout started at the very instant of the last one shown is skipped.
	let older = $state<string | null>(null);

	const day = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, {
			weekday: 'short',
			day: 'numeric',
			month: 'short',
			year: 'numeric'
		});
	const clock = (iso: string) =>
		new Date(iso).toLocaleTimeString(undefined, { timeStyle: 'short' });
</script>

{#if list.isPending}
	<div class="grid gap-2">
		{#each { length: first ? 5 : 2 }, i (i)}<Skeleton class="h-16" />{/each}
	</div>
{:else if list.isError}
	<p class="text-sm text-destructive">Workouts could not be loaded: {list.error.message}</p>
{:else if first && !list.data.length}
	<EmptyState {...empty} />
{:else}
	<ul class="grid gap-2">
		{#each list.data as w (w.workout_id)}
			<li>
				<a
					href={resolve('/workouts/[id]', { id: String(w.workout_id) })}
					class="grid gap-1 rounded-lg border p-3 transition-colors hover:bg-accent"
				>
					<span class="flex flex-wrap items-baseline justify-between gap-x-3">
						<span class="font-medium">{day(w.started_at)} · {w.gym.name}</span>
						<span class="text-sm text-muted-foreground tabular-nums"
							>{clock(w.started_at)}–{w.finished_at ? clock(w.finished_at) : 'now'} · {w.set_count}
							{w.set_count === 1 ? 'set' : 'sets'}</span
						>
					</span>
					{#if w.exercises.length}
						<span class="truncate text-sm text-muted-foreground"
							>{w.exercises.map((e) => e.exercise).join(', ')}</span
						>
					{/if}
				</a>
			</li>
		{/each}
	</ul>
	{#if older}
		<div class="mt-2"><WorkoutChunk {filters} to={older} {empty} /></div>
	{:else if list.data.length === LIMIT}
		<Button variant="outline" class="mt-3" onclick={() => (older = list.data.at(-1)!.started_at)}
			>Load older</Button
		>
	{/if}
{/if}
