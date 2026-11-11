<script lang="ts">
	import ExercisePicker from '#lib/components/exercise-picker.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import type { Exercise, Workout } from '#lib/api/results.js';
	import type { SetAddArgs } from '#lib/api/tools.gen.js';
	import { SET_TYPES, addSetsArgs, newSet, setTypeLabel, type NewSet } from '#lib/workouts.js';

	/** Pick an exercise, fill in one row per set, and add them to `workout` through `onadd`. */
	let {
		workout,
		onadd
	}: { workout: Workout; onadd: (args: Omit<SetAddArgs, 'owner'>) => Promise<unknown> } = $props();

	let exercise = $state<Exercise | null>(null);
	let rows = $state<NewSet[]>([newSet()]);
	let unilateral = $state(false);
	let problem = $state('');
	let busy = $state(false);
	let cardio = $derived(exercise?.kind === 'cardio');

	function pick(e: Exercise) {
		exercise = e;
		rows = [newSet()];
		unilateral = false;
		problem = '';
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!exercise) return;
		const r = addSetsArgs(exercise.id, rows, workout, unilateral);
		if ('error' in r) {
			problem = r.error;
			return;
		}
		problem = '';
		busy = true;
		try {
			await onadd(r.args);
			exercise = null;
		} catch {
			// The toast says why; the rows stay to fix them.
		} finally {
			busy = false;
		}
	}
</script>

<section class="grid gap-3 rounded-lg border p-4" aria-labelledby="add-sets-title">
	<h2 id="add-sets-title" class="font-semibold">Add sets</h2>
	{#if !exercise}
		<ExercisePicker id="add-sets-exercise" label="Exercise" onpick={pick} />
	{:else}
		<form class="grid gap-3" onsubmit={submit}>
			<div class="flex items-center gap-2">
				<p class="font-medium">{exercise.name}</p>
				<Button variant="ghost" size="sm" onclick={() => (exercise = null)}>Change exercise</Button>
			</div>
			<ol class="grid gap-3">
				{#each rows as row, i (i)}
					{@const id = `new-set-${i}`}
					<li class="grid grid-cols-2 items-end gap-2 border-b pb-3 sm:flex sm:flex-wrap">
						<span class="col-span-2 text-sm text-muted-foreground sm:w-12 sm:self-center"
							>Set {i + 1}</span
						>
						{#if cardio}
							<div class="grid gap-1.5">
								<Label for="{id}-duration">Duration (min)</Label>
								<Input
									id="{id}-duration"
									type="number"
									min="0"
									step="any"
									class="tabular-nums sm:w-28"
									bind:value={row.duration_min}
								/>
							</div>
							<div class="grid gap-1.5">
								<Label for="{id}-distance">Distance (km)</Label>
								<Input
									id="{id}-distance"
									type="number"
									min="0"
									step="any"
									class="tabular-nums sm:w-28"
									bind:value={row.distance_km}
								/>
							</div>
						{:else}
							<div class="grid gap-1.5">
								<Label for="{id}-reps">Reps</Label>
								<Input
									id="{id}-reps"
									type="number"
									min="0"
									step="1"
									class="tabular-nums sm:w-20"
									bind:value={row.reps}
								/>
							</div>
							<div class="grid gap-1.5">
								<Label for="{id}-weight">Weight (kg)</Label>
								<Input
									id="{id}-weight"
									type="number"
									min="0"
									step="any"
									class="tabular-nums sm:w-24"
									bind:value={row.weight_kg}
								/>
							</div>
						{/if}
						<div class="grid gap-1.5">
							<Label for="{id}-rpe">RPE</Label>
							<Input
								id="{id}-rpe"
								type="number"
								min="1"
								max="10"
								step="0.5"
								class="tabular-nums sm:w-20"
								bind:value={row.rpe}
							/>
						</div>
						<div class="grid gap-1.5">
							<Label for="{id}-type">Set type</Label>
							<Select.Root type="single" bind:value={row.set_type}>
								<Select.Trigger id="{id}-type" class="w-full sm:w-32"
									>{setTypeLabel(row.set_type)}</Select.Trigger
								>
								<Select.Content>
									{#each SET_TYPES as t (t.value)}<Select.Item
											value={t.value}
											label={t.label}
										/>{/each}
								</Select.Content>
							</Select.Root>
						</div>
						{#if workout.finished_at}
							<div class="col-span-2 grid gap-1.5">
								<Label for="{id}-time">Time</Label>
								<Input id="{id}-time" type="datetime-local" class="w-fit" bind:value={row.time} />
							</div>
						{/if}
						{#if rows.length > 1}
							<Button
								variant="ghost"
								size="sm"
								aria-label="Remove set {i + 1}"
								onclick={() => rows.splice(i, 1)}>Remove</Button
							>
						{/if}
					</li>
				{/each}
			</ol>
			<div class="flex flex-wrap items-center gap-4">
				<Button
					variant="outline"
					disabled={rows.length >= 50}
					onclick={() => rows.push(newSet(rows.at(-1)))}>Add another set</Button
				>
				{#if !cardio}
					<div class="flex items-center gap-2">
						<Checkbox id="new-sets-unilateral" bind:checked={unilateral} />
						<Label for="new-sets-unilateral" class="font-normal"
							>One side at a time (reps per side)</Label
						>
					</div>
				{/if}
			</div>
			{#if problem}<p class="text-sm text-destructive" role="alert">{problem}</p>{/if}
			<div>
				<Button type="submit" disabled={busy}
					>{rows.length === 1 ? 'Add set' : `Add ${rows.length} sets`}</Button
				>
			</div>
		</form>
	{/if}
</section>
