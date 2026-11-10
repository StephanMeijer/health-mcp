<script lang="ts">
	import { resolve } from '$app/paths';
	import PageHeader from '#lib/components/page-header.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ExerciseForm from '#lib/components/exercise-form.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { Exercise } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { COARSE_GROUPS, MUSCLE_GROUPS, muscleLabel } from '#lib/muscles.js';

	const ALL = 'all';
	let text = $state('');
	let query = $state('');
	let muscle = $state(ALL);
	let open = $state(false);

	$effect(() => {
		const q = text.trim();
		const t = setTimeout(() => (query = q), 250);
		return () => clearTimeout(t);
	});

	const exercises = toolQuery<Exercise[], 'exercise_search'>('exercise_search', () => ({
		query: query || null,
		muscle_group: muscle === ALL ? null : muscle
	}));
	const create = toolMutation<Exercise, 'exercise_create'>('exercise_create', {
		invalidate: ['exercise_search'],
		success: (e) => `${e.name} added`
	});
</script>

<PageHeader
	title="Exercises"
	description="Everything you log sets for, with the muscles it trains."
>
	{#snippet actions()}
		{#if logbook.canWrite('gym')}
			<Button onclick={() => (open = true)}>New exercise</Button>
		{/if}
	{/snippet}
</PageHeader>
<ReadOnlyNote domain="gym" />

<div class="mb-4 flex flex-wrap items-end gap-3">
	<div class="grid min-w-64 flex-1 gap-2">
		<Label for="exercise-search">Search</Label>
		<Input id="exercise-search" type="search" bind:value={text} placeholder="Bench press" />
	</div>
	<div class="grid gap-2">
		<Label for="muscle-filter">Muscle group</Label>
		<Select.Root type="single" bind:value={muscle}>
			<Select.Trigger id="muscle-filter" class="w-48"
				>{muscle === ALL ? 'All muscle groups' : muscleLabel(muscle)}</Select.Trigger
			>
			<Select.Content>
				<Select.Item value={ALL}>All muscle groups</Select.Item>
				<Select.Group>
					<Select.GroupHeading>Areas</Select.GroupHeading>
					{#each Object.keys(COARSE_GROUPS) as m (m)}
						<Select.Item value={m}>{muscleLabel(m)}</Select.Item>
					{/each}
				</Select.Group>
				<Select.Group>
					<Select.GroupHeading>Muscles</Select.GroupHeading>
					{#each MUSCLE_GROUPS as m (m)}
						<Select.Item value={m}>{muscleLabel(m)}</Select.Item>
					{/each}
				</Select.Group>
			</Select.Content>
		</Select.Root>
	</div>
</div>

{#if exercises.isPending}
	<div class="grid gap-2">
		{#each [1, 2, 3, 4] as i (i)}<Skeleton class="h-16 w-full" />{/each}
	</div>
{:else if exercises.isError}
	<p class="text-sm text-destructive">Couldn’t load exercises: {exercises.error.message}</p>
{:else if exercises.data.length === 0}
	{#if query || muscle !== ALL}
		<EmptyState
			title="No exercise matches"
			hint="Try another name or muscle group, or add the exercise if it's new."
		/>
	{:else}
		<EmptyState
			title="No exercises yet"
			hint="Exercises you log sets for show up here with the muscles they train. Add one, or let Claude create them as you log."
		>
			{#snippet action()}
				{#if logbook.canWrite('gym')}<Button onclick={() => (open = true)}>New exercise</Button
					>{/if}
			{/snippet}
		</EmptyState>
	{/if}
{:else}
	<ul class="divide-y rounded-lg border">
		{#each exercises.data as e (e.id)}
			<li class="grid gap-1.5 p-3">
				<div class="flex flex-wrap items-center gap-2">
					<a
						class="font-medium hover:underline"
						href={resolve('/exercises/[id]', { id: String(e.id) })}>{e.name}</a
					>
					<Badge variant="secondary">{e.kind === 'cardio' ? 'Cardio' : 'Strength'}</Badge>
				</div>
				{#if e.muscle_groups.length || e.secondary_muscle_groups.length}
					<div class="flex flex-wrap gap-1">
						{#each e.muscle_groups as m (m)}<Badge>{muscleLabel(m)}</Badge>{/each}
						{#each e.secondary_muscle_groups as m (m)}<Badge variant="outline"
								>{muscleLabel(m)}</Badge
							>{/each}
					</div>
				{/if}
				<p class="text-sm text-muted-foreground">{e.equipment || 'Bodyweight'}</p>
			</li>
		{/each}
	</ul>
	<p class="mt-2 text-xs text-muted-foreground">
		Filled chips are the target muscles; outlined ones assist.
	</p>
{/if}

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
		<Dialog.Header>
			<Dialog.Title>New exercise</Dialog.Title>
			<Dialog.Description
				>Be specific enough to compare weights: “Dumbbell bench press”, not “Bench”.</Dialog.Description
			>
		</Dialog.Header>
		{#if open}
			<ExerciseForm
				submitLabel="Add exercise"
				pending={create.isPending}
				onsubmit={(f) => create.mutate(f, { onSuccess: () => (open = false) })}
			/>
		{/if}
	</Dialog.Content>
</Dialog.Root>
