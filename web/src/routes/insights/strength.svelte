<script lang="ts">
	import { resolve } from '$app/paths';
	import * as Select from '#lib/components/ui/select/index.js';
	import * as Table from '#lib/components/ui/table/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import EmptyState from '#lib/components/empty-state.svelte';
	import ExercisePicker from '#lib/components/exercise-picker.svelte';
	import EChart from '#lib/charts/echart.svelte';
	import { progressLines, setupName } from '#lib/charts/progress.js';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { Exercise, PersonalRecord, Progress } from '#lib/api/results.js';
	import type { ProgressArgs } from '#lib/api/tools.gen.js';
	import { daysAgo, num } from '#lib/vitals.js';

	const METRICS = [
		{ value: 'e1rm', label: 'Estimated 1RM', unit: 'kg' },
		{ value: 'top_weight', label: 'Heaviest set', unit: 'kg' },
		{ value: 'volume', label: 'Volume', unit: 'kg' },
		{ value: 'reps', label: 'Reps', unit: 'reps' }
	] as const;
	const BUCKETS = [
		{ value: 'day', label: 'Per day' },
		{ value: 'week', label: 'Per week' },
		{ value: 'month', label: 'Per month' }
	] as const;
	const RANGES = [
		{ value: '90', label: 'Last 3 months' },
		{ value: '182', label: 'Last 6 months' },
		{ value: '365', label: 'Last year' },
		{ value: 'all', label: 'All time' }
	];

	let exercise = $state<Exercise | null>(null);
	let metric = $state<ProgressArgs['metric']>('e1rm');
	let bucket = $state<NonNullable<ProgressArgs['bucket']>>('week');
	let range = $state('182');
	let m = $derived(METRICS.find((x) => x.value === metric)!);

	const progress = toolQuery<Progress, 'progress'>(
		'progress',
		() => ({
			exercise: exercise?.name,
			metric,
			bucket,
			from: range === 'all' ? undefined : daysAgo(Number(range))
		}),
		{ enabled: () => exercise !== null }
	);
	let lines = $derived(progressLines(progress.data?.series ?? []));

	const records = toolQuery<PersonalRecord[], 'personal_records'>('personal_records', () => ({
		exercise: exercise?.name
	}));
	// Cardio exercises only have cardio records, which this table doesn't show.
	let strength = $derived(
		(records.data ?? []).filter((r) => r.heaviest_set || r.best_e1rm || r.best_volume_workout)
	);

	let option = $derived({
		tooltip: {
			trigger: 'axis',
			valueFormatter: (v: number) => `${num(v)} ${m.unit}`
		},
		legend: { show: lines.length > 1, bottom: 0 },
		grid: { left: 8, right: 16, top: 32, bottom: lines.length > 1 ? 40 : 8, containLabel: true },
		xAxis: { type: 'time' },
		yAxis: { type: 'value', scale: true, name: m.unit },
		series: lines.map((l) => ({ type: 'line', name: l.name, data: l.points }))
	});

	const date = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });
	const workout = (id: number) => `${resolve('/workouts')}/${id}`;
</script>

<div class="mb-4 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
	<div class="sm:col-span-2 lg:col-span-1">
		{#if exercise}
			<div class="grid gap-2">
				<span class="text-sm font-medium">Exercise</span>
				<div class="flex items-center gap-2">
					<span class="truncate font-medium">{exercise.name}</span>
					<Button variant="ghost" size="sm" onclick={() => (exercise = null)}>Change</Button>
				</div>
			</div>
		{:else}
			<ExercisePicker onpick={(e) => (exercise = e)} />
		{/if}
	</div>
	<div class="grid gap-2">
		<Label for="metric">Measure</Label>
		<Select.Root type="single" bind:value={metric}>
			<Select.Trigger id="metric" class="w-full">{m.label}</Select.Trigger>
			<Select.Content>
				{#each METRICS as x (x.value)}<Select.Item value={x.value} label={x.label}
						>{x.label}</Select.Item
					>{/each}
			</Select.Content>
		</Select.Root>
	</div>
	<div class="grid gap-2">
		<Label for="bucket">Points</Label>
		<Select.Root type="single" bind:value={bucket}>
			<Select.Trigger id="bucket" class="w-full"
				>{BUCKETS.find((b) => b.value === bucket)?.label}</Select.Trigger
			>
			<Select.Content>
				{#each BUCKETS as b (b.value)}<Select.Item value={b.value} label={b.label}
						>{b.label}</Select.Item
					>{/each}
			</Select.Content>
		</Select.Root>
	</div>
	<div class="grid gap-2">
		<Label for="range">Period</Label>
		<Select.Root type="single" bind:value={range}>
			<Select.Trigger id="range" class="w-full"
				>{RANGES.find((r) => r.value === range)?.label}</Select.Trigger
			>
			<Select.Content>
				{#each RANGES as r (r.value)}<Select.Item value={r.value} label={r.label}
						>{r.label}</Select.Item
					>{/each}
			</Select.Content>
		</Select.Root>
	</div>
</div>

<section class="mb-8">
	{#if !exercise}
		<EmptyState
			title="Pick an exercise to see its progress"
			hint="Choose one above: a line per machine and grip shows how it progressed over the period."
		/>
	{:else if progress.isPending}
		<Skeleton class="h-72" />
	{:else if progress.isError}
		<p class="text-sm text-destructive">
			The progress of {exercise.name} could not be loaded: {progress.error.message}
		</p>
	{:else if !lines.length}
		<EmptyState
			title="No working sets of {exercise.name} in this period"
			hint="Points appear here once working sets are logged. Try a longer period."
		/>
	{:else}
		<EChart
			{option}
			label="{m.label} of {exercise.name} {BUCKETS.find(
				(b) => b.value === bucket
			)?.label.toLowerCase()}, in {m.unit}"
		/>
	{/if}
</section>

<section>
	<h2 class="mb-3 text-lg font-semibold">Personal records</h2>
	{#if records.isPending}
		<div class="grid gap-2">
			{#each { length: 4 }, i (i)}<Skeleton class="h-10" />{/each}
		</div>
	{:else if records.isError}
		<p class="text-sm text-destructive">
			Your records could not be loaded: {records.error.message}
		</p>
	{:else if !strength.length}
		<EmptyState
			title="No records yet"
			hint="Your heaviest set, best estimated 1RM and biggest workout volume per exercise show up here once you log working sets."
		/>
	{:else}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- workout() resolves; /workouts/[id] is typed once that page lands -->
		<Table.Root>
			<Table.Header>
				<Table.Row>
					<Table.Head>Exercise</Table.Head>
					<Table.Head>Setup</Table.Head>
					<Table.Head>Heaviest set</Table.Head>
					<Table.Head>Best estimated 1RM</Table.Head>
					<Table.Head>Biggest workout volume</Table.Head>
				</Table.Row>
			</Table.Header>
			<Table.Body>
				{#each strength as r (`${r.exercise}|${r.machine}|${r.variant}`)}
					<Table.Row>
						<Table.Cell class="font-medium">{r.exercise}</Table.Cell>
						<Table.Cell class="text-muted-foreground">{setupName(r)}</Table.Cell>
						<Table.Cell>
							{#if r.heaviest_set}
								<span class="tabular-nums"
									>{num(r.heaviest_set.weight_kg)} kg × {r.heaviest_set.reps}</span
								>
								<a
									href={workout(r.heaviest_set.workout_id)}
									class="block text-xs text-muted-foreground hover:underline"
									>{date(r.heaviest_set.performed_at)}</a
								>
							{:else}–{/if}
						</Table.Cell>
						<Table.Cell>
							{#if r.best_e1rm}
								<span class="tabular-nums">{num(r.best_e1rm.e1rm_kg)} kg</span>
								<a
									href={workout(r.best_e1rm.workout_id)}
									class="block text-xs text-muted-foreground hover:underline"
									>{num(r.best_e1rm.weight_kg)} kg × {r.best_e1rm.reps}, {date(
										r.best_e1rm.performed_at
									)}</a
								>
							{:else}–{/if}
						</Table.Cell>
						<Table.Cell>
							{#if r.best_volume_workout}
								<span class="tabular-nums">{num(r.best_volume_workout.volume_kg, 0)} kg</span>
								<a
									href={workout(r.best_volume_workout.workout_id)}
									class="block text-xs text-muted-foreground hover:underline"
									>{date(r.best_volume_workout.date)}</a
								>
							{:else}–{/if}
						</Table.Cell>
					</Table.Row>
				{/each}
			</Table.Body>
		</Table.Root>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/if}
</section>
