<script lang="ts">
	import { goto } from '$app/navigation';
	import PageHeader from '#lib/components/page-header.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ExercisePicker from '#lib/components/exercise-picker.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { Exercise, GymListItem, Workout } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { fromLocalInput } from '#lib/vitals.js';
	import { workoutHref } from '#lib/workouts.js';
	import CurrentWorkout from './current-workout.svelte';
	import WorkoutChunk from './workout-chunk.svelte';

	let canWrite = $derived(logbook.canWrite('gym'));
	const gyms = toolQuery<GymListItem[], 'gym_list'>('gym_list', () => ({}));

	const ALL = 'all';
	let gym = $state(ALL);
	let exercise = $state<Exercise | null>(null);
	let from = $state('');
	let to = $state('');
	let filters = $derived({
		...(gym !== ALL && { gym }),
		...(exercise && { exercise: String(exercise.id) }),
		...(from && { from })
	});
	let filtered = $derived(gym !== ALL || exercise !== null || from !== '' || to !== '');
	let empty = $derived(
		filtered
			? { title: 'No workouts match', hint: 'Change or clear the filters to see more.' }
			: {
					title: 'No workouts yet',
					hint: canWrite
						? 'Workouts you start here, or log with Claude, show up in this list. Start one above, or add one you forgot to log.'
						: 'Workouts show up here once they are logged.'
				}
	);

	// ---- add past workout
	const addPast = toolMutation<Workout, 'workout_add_past'>('workout_add_past', {
		invalidate: ['workout_list', 'gym_list'],
		success: 'Workout added'
	});
	let pastOpen = $state(false);
	let past = $state({ gym: '', start: '', end: '', notes: '', estimated: false });
	let pastInvalid = $derived(past.start !== '' && past.end !== '' && past.end < past.start);

	function submitPast(e: SubmitEvent) {
		e.preventDefault();
		if (pastInvalid || !past.gym) return;
		addPast.mutate(
			{
				gym: past.gym,
				started_at: fromLocalInput(past.start),
				finished_at: fromLocalInput(past.end),
				...(past.notes.trim() && { notes: past.notes.trim() }),
				...(past.estimated && { time_estimated: true })
			},
			{
				onSuccess: (w) => {
					pastOpen = false;
					goto(workoutHref(w));
				}
			}
		);
	}
</script>

<PageHeader title="Workouts" description="Every session with its sets, newest first.">
	{#snippet actions()}
		{#if canWrite}
			<Button
				variant="outline"
				onclick={() => {
					past = { gym: '', start: '', end: '', notes: '', estimated: false };
					pastOpen = true;
				}}>Add past workout</Button
			>
		{/if}
	{/snippet}
</PageHeader>
<ReadOnlyNote domain="gym" />
<CurrentWorkout />

<div class="mb-4 flex flex-wrap items-end gap-3">
	<div class="grid gap-1.5">
		<Label for="filter-gym">Gym</Label>
		<Select.Root type="single" bind:value={gym}>
			<Select.Trigger id="filter-gym" class="w-48"
				>{gym === ALL
					? 'All gyms'
					: (gyms.data?.find((g) => String(g.id) === gym)?.name ?? 'All gyms')}</Select.Trigger
			>
			<Select.Content>
				<Select.Item value={ALL} label="All gyms" />
				{#each gyms.data ?? [] as g (g.id)}<Select.Item
						value={String(g.id)}
						label={g.name}
					/>{/each}
			</Select.Content>
		</Select.Root>
	</div>
	<div class="w-60 max-w-full">
		{#if exercise}
			<div class="grid gap-1.5">
				<span class="text-sm font-medium">Exercise</span>
				<div class="flex h-8 items-center gap-1">
					<span class="truncate text-sm">{exercise.name}</span>
					<Button variant="ghost" size="sm" onclick={() => (exercise = null)}>Any exercise</Button>
				</div>
			</div>
		{:else}
			<ExercisePicker id="filter-exercise" label="Exercise" onpick={(e) => (exercise = e)} />
		{/if}
	</div>
	<div class="grid gap-1.5">
		<Label for="filter-from">From</Label>
		<Input id="filter-from" type="date" bind:value={from} class="w-40" />
	</div>
	<div class="grid gap-1.5">
		<Label for="filter-to">Before</Label>
		<Input id="filter-to" type="date" bind:value={to} class="w-40" />
	</div>
</div>

{#key JSON.stringify([filters, to])}
	<WorkoutChunk {filters} to={to || null} first {empty} />
{/key}

<Dialog.Root bind:open={pastOpen}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Add past workout</Dialog.Title>
			<Dialog.Description
				>A session you forgot to log. Add its sets on the next page.</Dialog.Description
			>
		</Dialog.Header>
		<form class="grid gap-4" onsubmit={submitPast}>
			<div class="grid gap-2">
				<Label for="past-gym">Gym</Label>
				<Select.Root type="single" bind:value={past.gym}>
					<Select.Trigger id="past-gym" class="w-full"
						>{gyms.data?.find((g) => String(g.id) === past.gym)?.name ??
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
			<div class="grid gap-3 sm:grid-cols-2">
				<div class="grid gap-2">
					<Label for="past-start">Start</Label>
					<Input id="past-start" type="datetime-local" required bind:value={past.start} />
				</div>
				<div class="grid gap-2">
					<Label for="past-end">Finish</Label>
					<Input
						id="past-end"
						type="datetime-local"
						required
						bind:value={past.end}
						aria-invalid={pastInvalid}
						aria-describedby={pastInvalid ? 'past-end-error' : undefined}
					/>
				</div>
			</div>
			{#if pastInvalid}
				<p id="past-end-error" class="text-sm text-destructive">The finish is before the start.</p>
			{/if}
			<div class="grid gap-2">
				<Label for="past-notes">Notes</Label>
				<Textarea id="past-notes" bind:value={past.notes} />
			</div>
			<div class="flex items-center gap-2">
				<Checkbox id="past-estimated" bind:checked={past.estimated} />
				<Label for="past-estimated" class="font-normal">The times are estimates</Label>
			</div>
			<Dialog.Footer>
				<Button type="submit" disabled={addPast.isPending || !past.gym || pastInvalid}
					>Add workout</Button
				>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
