<script lang="ts">
	import type { Workout } from '#lib/api/results.js';
	import { REGION_COLOR, chipText, regionOf, type ExerciseInfo } from '#lib/charts/workout.js';
	import { muscleLabel } from '#lib/muscles.js';
	import { setTypeLabel } from '#lib/workouts.js';

	/** Every set as a chip per exercise, coloured by body region; a chip scrolls to its row in the set list. */
	let { workout, info }: { workout: Workout; info: Map<number, ExerciseInfo> } = $props();

	const show = (id: number) =>
		document.getElementById(`set-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
</script>

<div class="grid gap-3">
	{#each workout.exercises as ex (ex.exercise_id)}
		{@const i = info.get(ex.exercise_id)}
		{@const region = regionOf(i)}
		<div class="grid gap-1.5 sm:grid-cols-[11rem_1fr] sm:gap-3">
			<div class="text-sm leading-tight sm:text-right">
				<p class="font-medium">{ex.exercise}</p>
				{#if i?.muscle_groups.length}
					<p class="text-xs text-muted-foreground">
						{i.muscle_groups.map(muscleLabel).join(', ')}
					</p>
				{/if}
			</div>
			<ul
				class="flex flex-wrap gap-1.5"
				style:--c="var(--{region ? REGION_COLOR[region] : 'chart-5'})"
			>
				{#each ex.sets as s (s.set_id)}
					{@const off = s.set_type !== 'working'}
					<li>
						<button
							type="button"
							onclick={() => show(s.set_id)}
							aria-label="{ex.exercise}, set {s.set_number}: {chipText(s)}{off
								? `, ${setTypeLabel(s.set_type).toLowerCase()}`
								: ''}. Show it in the list."
							class="min-w-20 rounded-md border px-2 py-1 text-xs font-medium tabular-nums hover:ring-2 hover:ring-ring/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none
								{s.set_type === 'warmup'
								? 'border-dashed border-[var(--c)] bg-[color-mix(in_oklab,var(--c)_8%,transparent)] text-muted-foreground'
								: 'border-[var(--c)] bg-[color-mix(in_oklab,var(--c)_28%,transparent)]'}"
						>
							{chipText(s)}{#if off}<span class="ml-1 font-normal text-muted-foreground"
									>{setTypeLabel(s.set_type).toLowerCase()}</span
								>{/if}
						</button>
					</li>
				{/each}
			</ul>
		</div>
	{/each}
</div>
