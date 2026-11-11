<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import EChart from '#lib/charts/echart.svelte';
	import {
		REGION_COLOR,
		REGION_NAMES,
		ZONES,
		blocks,
		bounds,
		exercisesSql,
		firstTimers,
		heartRateSql,
		hrLine,
		regionOf,
		regionShares,
		summary,
		ticks,
		zoneMinutes,
		type ExerciseInfo
	} from '#lib/charts/workout.js';
	import { toolQuery } from '#lib/api/query.svelte.js';
	import type { SqlRows, Workout } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import EverySet from './every-set.svelte';

	/** The training report's picture of one workout: numbers, heart rate with exercise blocks, every set, zones, regions. */
	let { workout }: { workout: Workout } = $props();

	let ids = $derived(workout.exercises.map((e) => e.exercise_id));
	const exercises = toolQuery<SqlRows<ExerciseInfo>, 'sql_query'>(
		'sql_query',
		() => ({ sql: exercisesSql(ids) }),
		{ enabled: () => ids.length > 0 }
	);
	let infoRows = $derived(exercises.data?.rows ?? []);
	let info = $derived(new Map(infoRows.map((r) => [r.id, r])));

	let vitals = $derived(logbook.canRead('vitals'));
	let span = $derived(bounds(workout));
	const heart = toolQuery<SqlRows<{ t: number; bpm: number }>, 'sql_query'>(
		'sql_query',
		() => ({ sql: heartRateSql(...span) }),
		{ enabled: () => vitals && ids.length > 0 }
	);
	let rows = $derived(heart.data?.rows ?? []);
	let bpms = $derived(rows.map((r) => r.bpm));
	let avg = $derived(bpms.length ? Math.round(bpms.reduce((a, b) => a + b, 0) / bpms.length) : 0);
	let peak = $derived(bpms.length ? Math.round(Math.max(...bpms)) : 0);

	let sum = $derived(summary(workout, info));
	let zones = $derived(zoneMinutes(bpms));
	let shares = $derived(regionShares(workout, info));
	let firsts = $derived.by(() => {
		const ids = firstTimers(workout, infoRows);
		return workout.exercises.filter((e) => ids.has(e.exercise_id));
	});
	let runs = $derived(blocks(workout));
	let width = $state(0);

	const clock = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { timeStyle: 'short' });
	const n = (x: number, digits = 0) =>
		x.toLocaleString(undefined, { maximumFractionDigits: digits });
	const regionOfId = (id: number) => regionOf(info.get(id)) ?? 'Core';

	let option = $derived.by(() => {
		const line = hrLine(rows);
		const lo = bpms.length ? Math.min(60, Math.floor((Math.min(...bpms) - 10) / 10) * 10) : 60;
		const hi = bpms.length ? Math.max(180, Math.ceil((peak + 10) / 10) * 10) : 200;
		const used = REGION_NAMES.filter((r) => runs.some((b) => regionOfId(b.exerciseId) === r));
		// As in the report, each exercise is named once, at its first block, staggered over three rows. On a phone
		// they'd pile up, so the legend and the set grid name them instead.
		const labelled = width >= 640;
		const mid = (span[0] + span[1]) / 2;
		const peakLate = (line.find(([, v]) => v === peak)?.[0] ?? 0) > mid;
		const heads = runs.flatMap((b, i) =>
			runs.findIndex((x) => x.exerciseId === b.exerciseId) === i ? [i] : []
		);
		const row = (i: number) => heads.indexOf(i) % 3; // -1: not an exercise's first block
		return (token: (name: string) => string) => ({
			animation: false,
			legend: { bottom: 0, data: used, icon: 'roundRect' },
			grid: { left: 8, right: 16, top: labelled ? 56 : 32, bottom: 40, containLabel: true },
			tooltip: {
				show: line.length > 0,
				trigger: 'axis',
				formatter: (ps: { seriesName: string; value: [number, number | null] }[]) => {
					const p = ps.find((x) => x.seriesName === 'Heart rate');
					return p?.value[1] == null ? '' : `${clock(p.value[0])}: ${p.value[1]} bpm`;
				}
			},
			xAxis: {
				type: 'time',
				min: span[0],
				max: span[1],
				axisLabel: { formatter: '{HH}:{mm}', hideOverlap: true },
				splitLine: { show: false }
			},
			yAxis: { type: 'value', name: 'bpm', min: lo, max: hi, interval: 20 },
			series: [
				...used.map((region) => ({
					type: 'scatter',
					name: region,
					color: token(REGION_COLOR[region]),
					symbol: 'rect',
					symbolSize: [3, 12],
					symbolOffset: [0, -8],
					silent: true,
					data: ticks(workout)
						.filter((t) => regionOfId(t.exerciseId) === region)
						.map((t) => [t.at, lo]),
					markArea: {
						silent: true,
						itemStyle: { opacity: 0.55 },
						label: { show: labelled, color: token('muted-foreground'), fontSize: 11 },
						data: runs
							.map((b, i) => ({ b, i }))
							.filter(({ b }) => regionOfId(b.exerciseId) === region)
							.map(({ b, i }) => [
								{
									xAxis: b.from,
									name: b.exercise,
									label:
										row(i) < 0
											? { show: false }
											: {
													// Right of the middle the name runs leftwards, so the last ones stay inside the chart.
													position: b.from > mid ? 'insideTopRight' : 'insideTopLeft',
													offset: [0, -48 + 15 * row(i)]
												}
								},
								{ xAxis: b.to }
							])
					}
				})),
				...(line.length
					? [
							{
								type: 'line',
								name: 'Heart rate',
								color: token('foreground'),
								data: line,
								showSymbol: false,
								lineStyle: { width: 2 },
								z: 5,
								markPoint: {
									symbol: 'circle',
									symbolSize: 7,
									data: [{ type: 'max', name: 'Peak' }],
									label: {
										position: 'top',
										align: peakLate ? 'right' : 'left',
										color: token('foreground'),
										formatter: (p: { value: number }) => `peak ${p.value} bpm`
									}
								}
							}
						]
					: [])
			]
		});
	});
	let chartLabel = $derived(
		`Heart rate during the ${sum.minutes}-minute workout` +
			(bpms.length ? `: average ${avg} bpm, peak ${peak} bpm` : ', no heart rate recorded') +
			`; ${workout.set_count} sets in ${runs.length} exercise blocks.`
	);
	let zoneTotal = $derived(zones.reduce((a, b) => a + b, 0));
</script>

{#snippet tile(title: string, value: string, unit: string, sub: string, color: string)}
	<div class="rounded-lg border border-l-4 bg-card p-3" style:border-left-color="var(--{color})">
		<p class="text-xs text-muted-foreground">{title}</p>
		<p class="text-2xl font-semibold tabular-nums">
			{value}{#if unit}<span class="ml-1 text-sm font-normal text-muted-foreground">{unit}</span
				>{/if}
		</p>
		<p class="truncate text-xs text-muted-foreground" title={sub}>{sub}</p>
	</div>
{/snippet}

{#snippet bar(parts: { key: string; value: number; color: string; label: string }[], total: number)}
	<div class="flex h-4 overflow-hidden rounded-sm" aria-hidden="true">
		{#each parts as p (p.key)}
			{#if p.value}<div
					class="h-full border-r border-background last:border-r-0"
					style:width="{(p.value / total) * 100}%"
					style:background={p.color}
				></div>{/if}
		{/each}
	</div>
	<ul class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
		{#each parts as p (p.key)}
			<li class="flex items-center gap-1.5">
				<span class="size-2.5 rounded-xs" style:background={p.color} aria-hidden="true"></span>
				{p.label}
			</li>
		{/each}
	</ul>
{/snippet}

{#if exercises.isPending}
	<Skeleton class="h-24 w-full" />
	<Skeleton class="h-72 w-full" />
{:else if exercises.isError}
	<p class="text-sm text-destructive">
		The overview of this workout could not be loaded: {exercises.error.message}
	</p>
{:else}
	<div class="grid gap-8">
		<div class="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
			{@render tile(
				'Duration',
				n(sum.minutes),
				'min',
				`${clock(sum.from)}–${clock(sum.to)}`,
				'chart-3'
			)}
			{@render tile(
				'Working sets',
				n(sum.sets),
				'',
				`${sum.exercises} ${sum.exercises === 1 ? 'exercise' : 'exercises'}`,
				'chart-3'
			)}
			{@render tile(
				'Volume',
				n(sum.volumeKg),
				'kg',
				sum.carriedM ? `+ ${n(sum.carriedM)} m sled and carry` : 'reps × kg',
				'chart-3'
			)}
			{#if sum.machines.length}
				{@render tile(
					'Cardio',
					n(sum.cardioM / 1000, 2),
					'km',
					`${sum.cardioMin} min · ${sum.machines.join(', ')}`,
					'chart-1'
				)}
			{/if}
			{#if bpms.length}
				{@render tile('Heart rate', n(avg), 'avg', `max ${peak} bpm`, 'chart-4')}
			{/if}
		</div>

		<section class="grid gap-2" aria-labelledby="hr-title">
			<h3 id="hr-title" class="font-semibold">Heart rate during the workout</h3>
			{#if heart.isPending && vitals}
				<Skeleton class="h-80 w-full" />
			{:else}
				<div bind:clientWidth={width}>
					<EChart {option} label={chartLabel} class="h-80" />
				</div>
				<p class="text-xs text-muted-foreground">
					{#if !vitals}
						Heart rate is part of the vitals, which this logbook doesn't share with you.
					{:else if heart.isError}
						<span class="text-destructive"
							>Heart rate could not be loaded: {heart.error.message}</span
						>
					{:else if !rows.length}
						No heart rate was recorded during this workout.
					{:else}
						Average heart rate per minute.
					{/if}
					Coloured blocks are exercises by body region, ticks along the bottom the logged sets. Sets are
					timed when logged, so the blocks are approximate.
				</p>
			{/if}
		</section>

		<section class="grid gap-3" aria-labelledby="sets-title">
			<h3 id="sets-title" class="font-semibold">Every set</h3>
			<EverySet {workout} {info} />
		</section>

		{#if vitals && rows.length}
			<section class="grid gap-2" aria-labelledby="zones-title">
				<h3 id="zones-title" class="font-semibold">Time per heart-rate zone</h3>
				{@render bar(
					ZONES.map((z, i) => ({
						key: z.label,
						value: zones[i],
						color: `color-mix(in oklab, var(--chart-3) ${30 + i * 17.5}%, var(--background))`,
						label: `${z.label} bpm · ${zones[i]} min`
					})),
					zoneTotal
				)}
			</section>
		{/if}

		<section class="grid gap-2" aria-labelledby="regions-title">
			<h3 id="regions-title" class="font-semibold">Where the work went</h3>
			{#if shares.length}
				{@render bar(
					shares.map((s) => ({
						key: s.region,
						value: s.sets,
						color: `var(--${REGION_COLOR[s.region]})`,
						label: `${s.region} ${s.pct}% · ${n(s.sets, 1)} sets`
					})),
					shares.reduce((a, s) => a + s.sets, 0)
				)}
				<p class="text-xs text-muted-foreground">
					Working strength sets: a set counts fully for its main muscles and half for the ones that
					assist.
				</p>
			{:else}
				<p class="text-sm text-muted-foreground">No working strength sets in this workout.</p>
			{/if}
			{#if firsts.length}
				<div class="flex flex-wrap items-center gap-1.5 text-sm">
					<span class="text-muted-foreground">First time:</span>
					{#each firsts as f (f.exercise_id)}<Badge variant="secondary">{f.exercise}</Badge>{/each}
				</div>
			{/if}
		</section>
	</div>
{/if}
