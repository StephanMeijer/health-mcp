<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import BodyMap from '#lib/components/body-map.svelte';
	import EmptyState from '#lib/components/empty-state.svelte';
	import EChart from '#lib/charts/echart.svelte';
	import {
		isoDay,
		locationMuscles,
		regionSeries,
		sqlTimeZone,
		weekStarts
	} from '#lib/charts/volume.js';
	import { tool } from '#lib/api/client.js';
	import { toolKey, toolQuery } from '#lib/api/query.svelte.js';
	import type { BodyNote, MuscleGroupVolume, SqlRows } from '#lib/api/results.js';
	import { logbook } from '#lib/logbook.svelte.js';
	import { daysAgo, num } from '#lib/vitals.js';

	const WEEKS = 12;
	const weeks = weekStarts(WEEKS);
	const shortDate = (d: Date | string) =>
		new Date(d).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });

	// One muscle_group_volume call per week: the server's weighting (primary 1, secondary 0.5, per side) stays in one place.
	const weekly = createQuery(() => ({
		queryKey: [...toolKey('muscle_group_volume'), 'weekly', isoDay(weeks[0])],
		queryFn: () =>
			Promise.all(
				weeks.map(
					(w, i) =>
						tool(
							'muscle_group_volume',
							{
								from: isoDay(w),
								...(i < WEEKS - 1 && { to: isoDay(weeks[i + 1]) })
							},
							{ owner: logbook.owner }
						) as Promise<MuscleGroupVolume>
				)
			)
	}));
	let regions = $derived(regionSeries((weekly.data ?? []).map((w) => w.muscle_groups)));
	let weeklyOption = $derived({
		tooltip: { trigger: 'axis', valueFormatter: (v: number) => `${num(v)} sets` },
		legend: { bottom: 0 },
		grid: { left: 8, right: 8, top: 32, bottom: 56, containLabel: true },
		xAxis: { type: 'category', data: weeks.map(shortDate) },
		yAxis: { type: 'value', name: 'sets' },
		series: regions.map((r) => ({ type: 'bar', stack: 'sets', ...r }))
	});

	// Weeks start on Monday in UTC here; close enough for a weekly trend. A plain date parses as UTC, `T00:00` as local.
	const hrvSql = `SELECT week, coalesce(w.sets, 0) AS sets, round(h.rmssd::numeric, 1) AS avg_rmssd_ms FROM
  (SELECT date_trunc('week', performed_at) AS week, count(*) AS sets FROM workout_set
   WHERE set_type = 'working' AND performed_at >= date_trunc('week', now()) - interval '${WEEKS - 1} weeks' GROUP BY 1) w
  FULL JOIN (SELECT date_trunc('week', time) AS week, avg(rmssd_ms) AS rmssd FROM hrv
   WHERE time >= date_trunc('week', now()) - interval '${WEEKS - 1} weeks' GROUP BY 1) h USING (week)
ORDER BY 1`;
	const hrv = toolQuery<
		SqlRows<{ week: string; sets: number; avg_rmssd_ms: number | null }>,
		'sql_query'
	>('sql_query', () => ({ sql: hrvSql }), { enabled: () => logbook.canRead('vitals') });
	let hrvRows = $derived(hrv.data?.rows ?? []);
	let hrvOption = $derived({
		tooltip: { trigger: 'axis' },
		legend: { bottom: 0 },
		grid: { left: 8, right: 8, top: 32, bottom: 40, containLabel: true },
		xAxis: {
			type: 'category',
			data: hrvRows.map((r) => shortDate(`${r.week.slice(0, 10)}T00:00`))
		},
		yAxis: [
			{ type: 'value', name: 'sets' },
			{ type: 'value', name: 'ms', scale: true, splitLine: { show: false } }
		],
		series: [
			{
				type: 'bar',
				name: 'Working sets',
				data: hrvRows.map((r) => r.sets),
				tooltip: { valueFormatter: (v: number) => `${v} sets` }
			},
			{
				type: 'line',
				name: 'Average HRV',
				yAxisIndex: 1,
				connectNulls: true,
				data: hrvRows.map((r) => r.avg_rmssd_ms),
				tooltip: { valueFormatter: (v: number | null) => (v == null ? '–' : `${num(v)} ms`) }
			}
		]
	});

	const yearAgo = new Date(daysAgo(364));
	const tz = sqlTimeZone();
	const daily = toolQuery<SqlRows<{ day: string; sets: number }>, 'sql_query'>('sql_query', () => ({
		sql: `SELECT (performed_at AT TIME ZONE '${tz}')::date AS day, count(*) AS sets FROM workout_set
WHERE set_type = 'working' AND performed_at >= '${isoDay(yearAgo)}'::date GROUP BY 1 ORDER BY 1`
	}));
	let days = $derived(daily.data?.rows ?? []);
	let calendarOption = $derived({
		tooltip: {
			formatter: (p: { value: [string, number] }) =>
				`${new Date(`${p.value[0]}T00:00`).toLocaleDateString(undefined, { dateStyle: 'medium' })}: ${p.value[1]} working sets`
		},
		visualMap: {
			min: 1,
			max: Math.max(1, ...days.map((d) => d.sets)),
			orient: 'horizontal',
			left: 'center',
			bottom: 0,
			itemHeight: 120,
			text: ['More', 'Less']
		},
		calendar: {
			range: [isoDay(yearAgo), isoDay(new Date())],
			cellSize: ['auto', 16],
			dayLabel: { firstDay: 1, fontSize: 10 },
			left: 32,
			right: 8,
			top: 24
		},
		series: [
			{ type: 'heatmap', coordinateSystem: 'calendar', data: days.map((d) => [d.day, d.sets]) }
		]
	});

	const week = toolQuery<MuscleGroupVolume, 'muscle_group_volume'>('muscle_group_volume', () => ({
		days: 7
	}));
	const notes = toolQuery<BodyNote[], 'body_note_list'>('body_note_list', () => ({
		from: daysAgo(14)
	}));
	let sets = $derived(
		Object.fromEntries((week.data?.muscle_groups ?? []).map((m) => [m.muscle_group, m.sets]))
	);
	let soreness = $derived.by(() => {
		const sore: Record<string, number | null> = {};
		const unmatched: BodyNote[] = [];
		for (const n of notes.data ?? []) {
			const muscles = locationMuscles(n.location);
			if (!muscles.length) unmatched.push(n);
			for (const m of muscles) sore[m] = Math.max(sore[m] ?? 0, n.severity ?? 0) || null;
		}
		return { sore, unmatched };
	});
</script>

<div class="grid gap-6">
	<Card.Root>
		<Card.Header>
			<Card.Title>This week on your body</Card.Title>
			<Card.Description>
				Sets per muscle in the last 7 days, and where you noted soreness in the last 14 days. Around
				10–20 sets a week suits building muscle.
			</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if week.isPending || notes.isPending}
				<Skeleton class="mx-auto h-72 max-w-md" />
			{:else if week.isError}
				<p class="text-sm text-destructive">
					Your sets this week could not be loaded: {week.error.message}
				</p>
			{:else}
				<BodyMap {sets} sore={soreness.sore} />
				{#if notes.isError}
					<p class="mt-3 text-sm text-destructive">
						Sore spots could not be loaded: {notes.error.message}
					</p>
				{:else if soreness.unmatched.length}
					<div class="mt-3 text-sm">
						<p class="text-muted-foreground">Also noted, not on the map:</p>
						<ul class="mt-1 grid gap-0.5">
							{#each soreness.unmatched as n (n.id)}
								<li>
									{n.location}{#if n.severity}<span class="ml-1 text-muted-foreground tabular-nums"
											>· {n.severity}/10</span
										>{/if}
									<span class="text-muted-foreground"> · {shortDate(n.noted_at)}</span>
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			{/if}
		</Card.Content>
	</Card.Root>

	<Card.Root>
		<Card.Header>
			<Card.Title>Sets per week</Card.Title>
			<Card.Description>
				Last {WEEKS} weeks by body region. A set counts fully for the muscles it targets and half for
				the ones that assist.
			</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if weekly.isPending}
				<Skeleton class="h-80" />
			{:else if weekly.isError}
				<p class="text-sm text-destructive">
					Your weekly sets could not be loaded: {weekly.error.message}
				</p>
			{:else if !regions.length}
				<EmptyState
					title="No working sets in the last {WEEKS} weeks"
					hint="Bars per week appear here once you log working sets of strength exercises."
				/>
			{:else}
				<EChart
					option={weeklyOption}
					label="Weighted sets per week by body region, last {WEEKS} weeks"
					class="h-80"
				/>
			{/if}
		</Card.Content>
	</Card.Root>

	<Card.Root>
		<Card.Header>
			<Card.Title>Training and HRV</Card.Title>
			<Card.Description>
				Working sets per week next to your average HRV (RMSSD) that week. A falling HRV under a
				rising load can mean you need more rest.
			</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if !logbook.canRead('vitals')}
				<p class="text-sm text-muted-foreground">
					HRV is part of the vitals, which this logbook doesn’t share with you.
				</p>
			{:else if hrv.isPending}
				<Skeleton class="h-72" />
			{:else if hrv.isError}
				<p class="text-sm text-destructive">
					Your sets and HRV could not be loaded: {hrv.error.message}
				</p>
			{:else if !hrvRows.length}
				<EmptyState
					title="No sets or HRV in the last {WEEKS} weeks"
					hint="Log working sets, or sync a heart rate strap that measures HRV, and the weeks show up here."
				/>
			{:else}
				<EChart
					option={hrvOption}
					label="Working sets per week and average HRV in milliseconds, last {WEEKS} weeks"
				/>
			{/if}
		</Card.Content>
	</Card.Root>

	<Card.Root>
		<Card.Header>
			<Card.Title>Training calendar</Card.Title>
			<Card.Description>Working sets per day over the last year.</Card.Description>
		</Card.Header>
		<Card.Content>
			{#if daily.isPending}
				<Skeleton class="h-52" />
			{:else if daily.isError}
				<p class="text-sm text-destructive">
					Your training days could not be loaded: {daily.error.message}
				</p>
			{:else if !days.length}
				<EmptyState
					title="No training days in the last year"
					hint="Each day you log working sets gets a square here, darker the more sets."
				/>
			{:else}
				<!-- A year is 53 columns: scroll it sideways on a phone, starting at the latest days, rather than shrink the squares. -->
				<div
					class="overflow-x-auto"
					{@attach (el) => {
						const f = requestAnimationFrame(() => (el.scrollLeft = el.scrollWidth));
						return () => cancelAnimationFrame(f);
					}}
				>
					<EChart
						option={calendarOption}
						label="Working sets per day over the last year, {days.length} training days"
						class="h-52 min-w-[680px]"
					/>
				</div>
			{/if}
		</Card.Content>
	</Card.Root>
</div>
