<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Button, buttonVariants } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { errorJson } from '#lib/api/client.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type {
		GymListItem,
		Workout,
		WorkoutCurrent,
		WorkoutStartResult
	} from '#lib/api/results.js';
	import type { ToolName } from '#lib/api/tools.gen.js';
	import { logbook } from '#lib/logbook.svelte.js';

	/** The open workout (continue or finish it), or a gym to start one at. */
	const current = toolQuery<WorkoutCurrent, 'workout_current'>('workout_current', () => ({}));
	const gyms = toolQuery<GymListItem[], 'gym_list'>('gym_list', () => ({}));
	let canWrite = $derived(logbook.canWrite('gym'));

	const invalidate: ToolName[] = ['workout_current', 'workout_list', 'workout_get', 'gym_list'];
	const start = toolMutation<WorkoutStartResult, 'workout_start'>('workout_start', {
		invalidate,
		success: (r) => (r.status === 'resumed' ? 'Workout resumed' : 'Workout started')
	});
	const finish = toolMutation('workout_finish', { invalidate, success: 'Workout finished' });

	// The gym trained at most recently, until the person picks another.
	let chosen = $state('');
	let latest = $derived(
		gyms.data?.toSorted((a, b) =>
			(b.last_workout_at ?? '').localeCompare(a.last_workout_at ?? '')
		)[0]
	);
	let gym = $derived(chosen || (latest ? String(latest.id) : ''));
	/** The workout still open at another gym, when starting here ran into it. */
	let clash = $state<Workout | null>(null);

	async function begin(finish_open = false) {
		try {
			const r = await start.mutateAsync({ gym, ...(finish_open && { finish_open }) });
			clash = null;
			await goto(resolve('/workouts/[id]', { id: String(r.workout.workout_id) }));
		} catch (e) {
			clash = (errorJson(e)?.open_workout as Workout | undefined) ?? null;
		}
	}

	const clock = (iso: string) =>
		new Date(iso).toLocaleTimeString(undefined, { timeStyle: 'short' });
	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<section class="mb-8 grid gap-4 rounded-lg border bg-card p-4" aria-label="Current workout">
	{#if current.isPending}
		<Skeleton class="h-16 w-full" />
	{:else if current.isError}
		<p class="text-sm text-destructive">
			The current workout could not be loaded: {current.error.message}
		</p>
	{:else}
		{@const open = current.data.open_workout}
		{@const stale = current.data.closed_stale}
		{#if open}
			<div class="flex flex-wrap items-center gap-3">
				<div class="grid gap-0.5">
					<p class="font-medium">In progress at {open.gym.name}</p>
					<p class="text-sm text-muted-foreground tabular-nums">
						Since {clock(open.started_at)} · {open.set_count}
						{open.set_count === 1 ? 'set' : 'sets'}
					</p>
				</div>
				<div class="ml-auto flex gap-2">
					<a
						class={buttonVariants({ variant: canWrite ? 'outline' : 'default' })}
						href={resolve('/workouts/[id]', { id: String(open.workout_id) })}
						>{canWrite ? 'Continue workout' : 'Open workout'}</a
					>
					{#if canWrite}
						<Button onclick={() => finish.mutate({})} disabled={finish.isPending}
							>Finish workout</Button
						>
					{/if}
				</div>
			</div>
		{:else if !canWrite}
			<p class="text-sm text-muted-foreground">No workout in progress.</p>
		{/if}
		{#if stale}
			<p class="text-sm text-muted-foreground">
				The workout at {stale.gym} has had no activity since {when(stale.last_activity)}. It
				{stale.action.includes('deleted')
					? 'has no sets or notes, so it will be deleted'
					: `will be finished at its last set (${stale.sets} ${stale.sets === 1 ? 'set' : 'sets'})`}
				when the next workout starts.
			</p>
		{/if}
		{#if canWrite}
			{#if gyms.data?.length === 0}
				<p class="text-sm text-muted-foreground">
					Add a gym first, on the Gyms page, to start a workout there.
				</p>
			{:else}
				<div class="flex flex-wrap items-end gap-3">
					<div class="grid gap-1.5">
						<Label for="start-gym">{open ? 'Or start at' : 'Start a workout at'}</Label>
						<Select.Root type="single" value={gym} onValueChange={(v) => (chosen = v)}>
							<Select.Trigger id="start-gym" class="w-56 max-w-full"
								>{gyms.data?.find((g) => String(g.id) === gym)?.name ??
									'Choose a gym'}</Select.Trigger
							>
							<Select.Content>
								{#each gyms.data ?? [] as g (g.id)}<Select.Item
										value={String(g.id)}
										label={g.name}
									/>{/each}
							</Select.Content>
						</Select.Root>
					</div>
					<Button onclick={() => begin()} disabled={!gym || start.isPending}>Start workout</Button>
				</div>
			{/if}
			{#if clash}
				<div class="grid gap-2 rounded-md border border-destructive/40 p-3" role="alert">
					<p class="text-sm">
						Your workout at {clash.gym.name} ({clash.set_count}
						{clash.set_count === 1 ? 'set' : 'sets'}) is still open. Finish it to start here, or
						keep it going.
					</p>
					<div class="flex flex-wrap gap-2">
						<Button onclick={() => begin(true)} disabled={start.isPending}
							>Finish it and start here</Button
						>
						<Button variant="ghost" onclick={() => (clash = null)}>Keep it open</Button>
					</div>
				</div>
			{/if}
		{/if}
	{/if}
</section>
