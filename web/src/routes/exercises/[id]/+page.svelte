<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageHeader from '#lib/components/page-header.svelte';
	import ReadOnlyNote from '#lib/components/read-only-note.svelte';
	import ExerciseForm from '#lib/components/exercise-form.svelte';
	import ExercisePicker from '#lib/components/exercise-picker.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { toolMutation, toolQuery } from '#lib/api/query.svelte.js';
	import type { Alternatives, Exercise, GymListItem } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { muscleLabel } from '#lib/muscles.js';

	let id = $derived(page.params.id ?? '');
	// No exercise_get: a search for the id returns exactly that exercise.
	const found = toolQuery<Exercise[], 'exercise_search'>('exercise_search', () => ({ query: id }));
	let exercise = $derived(found.data?.find((e) => String(e.id) === id));

	const update = toolMutation<Exercise, 'exercise_update'>('exercise_update', {
		invalidate: ['exercise_search', 'exercise_alternatives'],
		success: 'Exercise saved'
	});

	const CURRENT = 'current';
	let gym = $state(CURRENT);
	const gyms = toolQuery<GymListItem[], 'gym_list'>('gym_list', () => ({}));
	const alternatives = toolQuery<Alternatives, 'exercise_alternatives'>(
		'exercise_alternatives',
		() => ({ exercise: id, gym: gym === CURRENT ? null : gym }),
		{ enabled: () => exercise !== undefined }
	);
	let gymLabel = $derived(
		gym === CURRENT
			? 'Gym of the open workout'
			: (gyms.data?.find((g) => String(g.id) === gym)?.name ?? gym)
	);
</script>

<a
	href={resolve('/exercises')}
	class="mb-2 inline-block text-sm text-muted-foreground hover:underline">← All exercises</a
>
{#if found.isPending}
	<Skeleton class="mb-4 h-8 w-64" />
	<Skeleton class="h-64 w-full" />
{:else if found.isError}
	<p class="text-sm text-destructive">Couldn’t load this exercise: {found.error.message}</p>
{:else if !exercise}
	<p class="text-sm text-muted-foreground">
		This exercise doesn’t exist (any more). Pick one from the list.
	</p>
{:else}
	<PageHeader title={exercise.name} />
	<ReadOnlyNote domain="gym" />
	<div class="grid gap-10 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
		{#key exercise}
			<ExerciseForm
				{exercise}
				submitLabel="Save exercise"
				readonly={!logbook.canWrite('gym')}
				pending={update.isPending}
				onsubmit={(f) => update.mutate({ exercise: id, ...f })}
			/>
		{/key}
		<section class="grid content-start gap-4">
			<h2 class="text-lg font-semibold">Alternatives</h2>
			<p class="text-sm text-muted-foreground">
				Exercises that train the same target muscles, for when this one is taken.
			</p>
			<div class="grid gap-2">
				<Label for="alt-gym">At gym</Label>
				<Select.Root type="single" bind:value={gym}>
					<Select.Trigger id="alt-gym" class="w-full">{gymLabel}</Select.Trigger>
					<Select.Content>
						<Select.Item value={CURRENT}>Gym of the open workout</Select.Item>
						{#each gyms.data ?? [] as g (g.id)}
							<Select.Item value={String(g.id)}>{g.name}</Select.Item>
						{/each}
					</Select.Content>
				</Select.Root>
			</div>
			{#if alternatives.isPending}
				<Skeleton class="h-24 w-full" />
			{:else if alternatives.isError}
				<p class="text-sm text-destructive">
					Couldn’t load alternatives: {alternatives.error.message}
				</p>
			{:else}
				{@const a = alternatives.data}
				{#if a.gym?.equipment}
					<div class="rounded-md border bg-muted/50 p-3 text-sm">
						<p class="mb-1 font-medium">{a.gym.name} has</p>
						<p class="whitespace-pre-line text-muted-foreground">{a.gym.equipment}</p>
					</div>
				{/if}
				{#if a.alternatives.length === 0}
					<p class="text-sm text-muted-foreground">
						No other exercise targets the same muscles yet.
					</p>
				{:else}
					<ul class="divide-y rounded-lg border">
						{#each a.alternatives as alt (alt.id)}
							<li class="grid gap-1.5 p-3">
								<a
									class="font-medium hover:underline"
									href={resolve('/exercises/[id]', { id: String(alt.id) })}>{alt.name}</a
								>
								<div class="flex flex-wrap gap-1">
									{#each alt.shared_muscle_groups as m (m)}<Badge>{muscleLabel(m)}</Badge>{/each}
									{#each alt.shared_secondary as m (m)}<Badge variant="outline"
											>{muscleLabel(m)}</Badge
										>{/each}
								</div>
								<p class="text-sm text-muted-foreground">{alt.equipment || 'Bodyweight'}</p>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
			<ExercisePicker
				id="other-exercise"
				label="Alternatives for another exercise"
				onpick={(e) => goto(resolve('/exercises/[id]', { id: String(e.id) }))}
			/>
		</section>
	</div>
{/if}
