<script lang="ts" module>
	import type { MuscleGroup } from '#lib/muscles.js';

	type Spot = [cx: number, cy: number, rx: number, ry: number];
	// Left-hand spots on a 100×200 figure; a spot off the centre line is mirrored to the right side.
	const FRONT: [MuscleGroup, Spot][] = [
		['neck', [50, 26, 5, 4]],
		['front_delts', [31, 35, 5, 5]],
		['side_delts', [25, 37, 3.5, 5.5]],
		['chest', [41, 43, 8.5, 6]],
		['biceps', [24, 53, 4, 9]],
		['forearms', [21, 80, 4, 11]],
		['core', [50, 71, 10, 17]],
		['abductors', [35, 100, 3, 7]],
		['quads', [41.5, 124, 6, 17]],
		['adductors', [47.5, 114, 2, 10]]
	];
	const BACK: [MuscleGroup, Spot][] = [
		['neck', [50, 26, 5, 4]],
		['upper_back', [50, 40, 14, 8]],
		['rear_delts', [31, 35, 5, 5]],
		['side_delts', [25, 37, 3.5, 5.5]],
		['lats', [38, 56, 6, 12]],
		['lower_back', [50, 80, 8, 9]],
		['triceps', [24, 53, 4, 9]],
		['forearms', [21, 80, 4, 11]],
		['glutes', [42.5, 101, 7.5, 8]],
		['hamstrings', [41.5, 128, 6, 16]],
		['calves', [41.5, 164, 5, 11]]
	];
	const SHADE = [
		'fill-muted-foreground/15',
		'fill-chart-1/30',
		'fill-chart-1/60',
		'fill-chart-1',
		'fill-chart-2'
	];
</script>

<script lang="ts">
	import { muscleShade } from '#lib/charts/volume.js';
	import { MUSCLE_GROUPS, muscleLabel } from '#lib/muscles.js';
	import { num } from '#lib/vitals.js';

	/**
	 * Front and back of a body, each muscle group shaded by its weighted sets in the last 7 days; sore muscles
	 * (`sore`: muscle → worst severity, null when none was given) get a red outline.
	 */
	let { sets, sore }: { sets: Record<string, number>; sore: Record<string, number | null> } =
		$props();

	const describe = (m: MuscleGroup) =>
		`${muscleLabel(m)}: ${num(sets[m] ?? 0)} sets` +
		(m in sore ? `, sore${sore[m] ? ` (${sore[m]}/10)` : ''}` : '');
</script>

{#snippet figure(name: string, spots: [MuscleGroup, Spot][])}
	<figure class="grid justify-items-center gap-1">
		<svg viewBox="0 0 100 196" class="h-72 w-auto" role="img" aria-label="{name} of the body">
			<g class="fill-muted stroke-border" stroke-width="0.6">
				<circle cx="50" cy="13" r="9" />
				<rect x="45" y="20" width="10" height="10" rx="3" />
				<rect x="32" y="30" width="36" height="70" rx="9" />
				{#each [false, true] as right (right)}
					<g transform={right ? 'matrix(-1 0 0 1 100 0)' : undefined}>
						<rect x="19" y="32" width="10" height="40" rx="5" />
						<rect x="16" y="68" width="10" height="36" rx="5" />
						<rect x="34" y="94" width="15" height="98" rx="6" />
					</g>
				{/each}
			</g>
			{#each spots as [m, [cx, cy, rx, ry]] (m)}
				<g
					class="{SHADE[muscleShade(sets[m] ?? 0)]} {m in sore
						? 'stroke-destructive'
						: 'stroke-none'}"
					stroke-width="1.4"
				>
					<title>{describe(m)}</title>
					<ellipse {cx} {cy} {rx} {ry} />
					{#if cx !== 50}<ellipse cx={100 - cx} {cy} {rx} {ry} />{/if}
				</g>
			{/each}
		</svg>
		<figcaption class="text-xs text-muted-foreground">{name}</figcaption>
	</figure>
{/snippet}

<div class="flex flex-wrap justify-center gap-6">
	{@render figure('Front', FRONT)}
	{@render figure('Back', BACK)}
</div>

<div class="mt-3 flex flex-wrap justify-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
	{#each [['None', 0], ['Under 5', 1], ['5–10', 2], ['10–20', 3], ['Over 20', 4]] as const as [label, i] (i)}
		<span class="flex items-center gap-1.5">
			<svg class="size-3" aria-hidden="true"><circle cx="6" cy="6" r="6" class={SHADE[i]} /></svg>
			{label}
		</span>
	{/each}
	<span class="flex items-center gap-1.5">
		<svg class="size-3" aria-hidden="true"
			><circle cx="6" cy="6" r="5" class="fill-none stroke-destructive" stroke-width="2" /></svg
		>
		Sore
	</span>
</div>

<details class="mt-3 text-sm">
	<summary class="cursor-pointer text-muted-foreground">Show as a table</summary>
	<table class="mt-2 w-full text-left">
		<thead>
			<tr class="border-b"
				><th class="py-1 font-medium">Muscle</th><th class="font-medium">Sets</th><th
					class="font-medium">Sore</th
				></tr
			>
		</thead>
		<tbody>
			{#each MUSCLE_GROUPS as m (m)}
				<tr class="border-b last:border-0">
					<td class="py-1">{muscleLabel(m)}</td>
					<td class="tabular-nums">{num(sets[m] ?? 0)}</td>
					<td>{m in sore ? (sore[m] ? `Yes, ${sore[m]}/10` : 'Yes') : '–'}</td>
				</tr>
			{/each}
		</tbody>
	</table>
</details>
